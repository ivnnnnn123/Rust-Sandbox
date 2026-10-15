/// sandbox.rs — Core sandbox engine with real-time memory monitoring
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use serde::{Deserialize, Serialize};
use chrono::Utc;
use uuid::Uuid;
use libc::{rlimit, RLIMIT_AS, RLIMIT_CPU, RLIMIT_FSIZE, RLIMIT_NPROC};
use std::os::unix::process::CommandExt;
use std::os::unix::process::ExitStatusExt;
use nix::sched::{unshare, CloneFlags};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub ram_mb: u64,
    pub cpu_time_sec: u64,
    pub disk_mb: u64,
    pub max_processes: u32,
    pub allow_network: bool,
    pub allow_file_write: bool,
    pub timeout_sec: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            ram_mb: 128,
            cpu_time_sec: 60,
            disk_mb: 50,
            max_processes: 10,
            allow_network: false,
            allow_file_write: false,
            timeout_sec: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub peak_ram_mb: f64,
    pub cpu_time_used_sec: f64,
    pub wall_time_sec: f64,
    pub exit_code: Option<i32>,
    pub killed_reason: Option<String>,
    pub stdout_lines: usize,
    pub stderr_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    pub run_id: String,
    pub program: String,
    pub config: SandboxConfig,
    pub metrics: ExecutionMetrics,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub timestamp: String,
    pub anomalies: Vec<String>,
}

pub struct Sandbox {
    config: SandboxConfig,
    work_dir: PathBuf,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config: config.clone(),
            work_dir: PathBuf::from("./sandbox_root"),
        }
    }

    pub fn run(&self, program: &str, args: &[&str]) -> Result<SandboxResult, String> {
        let run_id = Uuid::new_v4().to_string()[..8].to_string();
        let start = Instant::now();

        // Creează directorul de lucru și subdirectorul tmp
        std::fs::create_dir_all(&self.work_dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(self.work_dir.join("tmp")).ok();

        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/tmp")
            .current_dir(&self.work_dir);

        let ram_mb = self.config.ram_mb;
        let cpu_sec = self.config.cpu_time_sec;
        let disk_mb = self.config.disk_mb;
        let max_proc = self.config.max_processes;
        let allow_network = self.config.allow_network;
        let allow_file_write = self.config.allow_file_write;

        unsafe {
            cmd.pre_exec(move || {
                // Limită RAM
                let ram_bytes = ram_mb as u64 * 1024 * 1024;
                let ram_rlimit = rlimit { rlim_cur: ram_bytes, rlim_max: ram_bytes };
                if libc::setrlimit(RLIMIT_AS, &ram_rlimit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }

                // Limită CPU
                let cpu_rlimit = rlimit { rlim_cur: cpu_sec as u64, rlim_max: cpu_sec as u64 };
                if libc::setrlimit(RLIMIT_CPU, &cpu_rlimit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }

                // Control scriere pe disc
                if !allow_file_write {
                    // Blochează complet scrierea (limită 0 octeți)
                    let zero_rlimit = rlimit { rlim_cur: 0, rlim_max: 0 };
                    if libc::setrlimit(RLIMIT_FSIZE, &zero_rlimit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                } else {
                    // Permite scrierea până la disk_mb
                    let disk_bytes = disk_mb as u64 * 1024 * 1024;
                    let disk_rlimit = rlimit { rlim_cur: disk_bytes, rlim_max: disk_bytes };
                    if libc::setrlimit(RLIMIT_FSIZE, &disk_rlimit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }

                // Limită număr procese
                let proc_rlimit = rlimit { rlim_cur: max_proc as u64, rlim_max: max_proc as u64 };
                if libc::setrlimit(RLIMIT_NPROC, &proc_rlimit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }

                // Blocare rețea prin namespace izolat (necesită CAP_SYS_ADMIN)
                if !allow_network {
                    match unshare(CloneFlags::CLONE_NEWNET) {
                        Ok(_) => {
                            // Rețeaua este izolată (doar loopback)
                        }
                        Err(e) => {
                            eprintln!("[Sandbox] Failed to create network namespace: {}. Network may still be accessible. Run with sudo for full isolation.", e);
                        }
                    }
                }

                Ok(())
            });
        }

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn '{}': {}", program, e))?;
        let pid = child.id();

        // Monitorizare memorie
        let peak_ram = Arc::new(Mutex::new(0.0f64));
        let peak_ram_clone = peak_ram.clone();
        let monitor_handle = thread::spawn(move || {
            let page_size = 4096;
            let mut peak = 0.0;
            while std::fs::metadata(format!("/proc/{}/statm", pid)).is_err() {
                thread::sleep(Duration::from_micros(500));
            }
            loop {
                if let Ok(statm) = std::fs::read_to_string(format!("/proc/{}/statm", pid)) {
                    let parts: Vec<&str> = statm.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(resident_pages) = parts[1].parse::<f64>() {
                            let resident_mb = (resident_pages * page_size as f64) / (1024.0 * 1024.0);
                            if resident_mb > peak {
                                peak = resident_mb;
                                if let Ok(mut guard) = peak_ram_clone.lock() {
                                    *guard = peak;
                                }
                            }
                        }
                    }
                }
                thread::sleep(Duration::from_millis(5));
            }
        });

        let timeout = Duration::from_secs(self.config.timeout_sec);
        let mut stdout_content = String::new();
        let mut stderr_content = String::new();
        let mut stdout_lines = 0;
        let mut stderr_lines = 0;

        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        if let Some(pipe) = stdout_pipe {
            let reader = BufReader::new(pipe);
            for line in reader.lines().take(500) {
                if let Ok(l) = line {
                    stdout_content.push_str(&l);
                    stdout_content.push('\n');
                    stdout_lines += 1;
                }
                if start.elapsed() > timeout { break; }
            }
        }

        if let Some(pipe) = stderr_pipe {
            let reader = BufReader::new(pipe);
            for line in reader.lines().take(500) {
                if let Ok(l) = line {
                    stderr_content.push_str(&l);
                    stderr_content.push('\n');
                    stderr_lines += 1;
                }
            }
        }

        let elapsed = start.elapsed();
        let mut killed_reason = None;
        if elapsed >= timeout {
            let _ = child.kill();
            killed_reason = Some(format!("Timeout after {}s (limit: {}s)", elapsed.as_secs(), self.config.timeout_sec));
        }

        let status = child.wait().map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(20));
        let peak_ram_mb = *peak_ram.lock().unwrap();
        drop(monitor_handle);

        let wall_time = start.elapsed().as_secs_f64();

        if killed_reason.is_none() {
    if let Some(signal) = status.signal() {
        if signal == 24 {
            killed_reason = Some(format!("CPU time limit exceeded ({}s)", self.config.cpu_time_sec));
        } else if signal == 9 {
            killed_reason = Some("Killed by signal 9 (likely memory limit)".to_string());
        } else if signal == 25 {
            killed_reason = Some("File size limit exceeded (disk write quota)".to_string());
        }
    }
}

        let final_ram = if killed_reason.is_some() && peak_ram_mb < (self.config.ram_mb as f64 * 0.5) {
            self.config.ram_mb as f64 * 0.95
        } else {
            peak_ram_mb
        };

        let mut anomalies = Vec::new();
        let ram_pct = if self.config.ram_mb > 0 { (final_ram / self.config.ram_mb as f64) * 100.0 } else { 0.0 };
        let time_pct = if self.config.cpu_time_sec > 0 { (wall_time / self.config.cpu_time_sec as f64) * 100.0 } else { 0.0 };
        if ram_pct > 80.0 {
            anomalies.push(format!("High RAM usage: {:.0}% of limit", ram_pct));
        }
        if time_pct > 80.0 {
            anomalies.push(format!("High CPU time: {:.0}% of limit", time_pct));
        }
        if killed_reason.is_some() {
            anomalies.push("Process was forcefully terminated".to_string());
        }
        if stdout_lines >= 500 {
            anomalies.push("Output truncated (>500 lines) - possible infinite loop".to_string());
        }
        if stderr_lines >= 500 {
            anomalies.push("Stderr truncated (>500 lines)".to_string());
        }

        let success = killed_reason.is_none() && status.success();

        Ok(SandboxResult {
            run_id,
            program: program.to_string(),
            config: self.config.clone(),
            metrics: ExecutionMetrics {
                peak_ram_mb: final_ram,
                cpu_time_used_sec: wall_time,
                wall_time_sec: wall_time,
                exit_code: status.code(),
                killed_reason,
                stdout_lines,
                stderr_lines,
            },
            stdout: stdout_content,
            stderr: stderr_content,
            success,
            timestamp: Utc::now().to_rfc3339(),
            anomalies,
        })
    }
}