mod sandbox;
mod history;
mod ai;

use sandbox::{Sandbox, SandboxConfig};
use history::{History, HumanDecision};
use ai::AiAdvisor;
use std::env;
use std::io::{self, Write};

const HISTORY_FILE: &str = "history.json";

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: sandbox <program> [args...]");
        eprintln!("   or: sandbox history");
        eprintln!("   or: sandbox stats");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "history" => cmd_history(),
        "stats" => cmd_stats(),
        _ => {
            let program = &args[1];
            let prog_args: Vec<&str> = args[2..].iter().map(|s| s.as_str()).collect();
            run_program(program, &prog_args);
        }
    }
}

fn run_program(program: &str, args: &[&str]) {
    let mut history = History::load(HISTORY_FILE);
    let base_config = history.suggest_baseline_config(program).unwrap_or_default();

    println!("\n=== Sandbox Execution ===");
    println!("Program: {}", program);
    println!("Config: RAM={}MB, CPU={}s, Disk={}MB, Timeout={}s",
        base_config.ram_mb, base_config.cpu_time_sec, base_config.disk_mb, base_config.timeout_sec);

    let sandbox = Sandbox::new(base_config.clone());
    let result = match sandbox.run(program, args) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Sandbox error: {}", e);
            return;
        }
    };

    println!("\n--- Results ---");
    println!("Status: {}", if result.success { "SUCCESS" } else { "FAILED" });
    println!("RAM used: {:.1} MB / {} MB", result.metrics.peak_ram_mb, result.config.ram_mb);
    println!("Time: {:.2}s / {}s", result.metrics.wall_time_sec, result.config.cpu_time_sec);
    if let Some(ref k) = result.metrics.killed_reason {
        println!("Killed: {}", k);
    }
    if !result.stdout.is_empty() {
        println!("\n--- stdout ---\n{}", &result.stdout[..result.stdout.len().min(500)]);
    }
    if !result.anomalies.is_empty() {
        println!("\n--- Anomalies ---");
        for a in &result.anomalies { println!(" • {}", a); }
    }

    let advisor = AiAdvisor::new("".to_string());
    let recommendation = match advisor.analyze(&result, &history) {
        Ok(rec) => rec,
        Err(e) => {
            eprintln!("\n[AI] Error: {}", e);
            eprintln!("Cannot proceed without a valid Groq API recommendation.");
            std::process::exit(1);
        }
    };

    println!("\n--- AI Recommendation ---");
    println!("Severity: {}", recommendation.severity);
    println!("Action: {}", recommendation.action);
    println!("Reasoning: {}", recommendation.reasoning);
    println!("Suggested: RAM {} MB, CPU {} s", recommendation.suggested_ram_mb, recommendation.suggested_cpu_time_sec);

    print!("Accept? [a]ccept / [r]eject / [m]odify: ");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let choice = input.trim().to_lowercase();

    let (decision, effective) = match choice.as_str() {
        "a" => (Some(HumanDecision::Accepted), SandboxConfig {
            ram_mb: recommendation.suggested_ram_mb,
            cpu_time_sec: recommendation.suggested_cpu_time_sec,
            disk_mb: recommendation.suggested_disk_mb,
            timeout_sec: recommendation.suggested_timeout_sec,
            ..base_config
        }),
        "m" => {
            let new_ram = read_u64("New RAM MB: ", base_config.ram_mb);
            let new_cpu = read_u64("New CPU sec: ", base_config.cpu_time_sec);
            let new_cfg = SandboxConfig { ram_mb: new_ram, cpu_time_sec: new_cpu, ..base_config };
            (Some(HumanDecision::ModifiedAndAccepted { new_config: new_cfg.clone() }), new_cfg)
        }
        _ => (Some(HumanDecision::Rejected), base_config.clone()),
    };

    history.add_run(&result, Some(recommendation), decision, effective);
    if let Err(e) = history.save(HISTORY_FILE) {
        eprintln!("[HISTORY] Failed to save history: {}", e);
    }
    println!("Decision saved.");
}

fn read_u64(prompt: &str, default: u64) -> u64 {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut s = String::new();
    io::stdin().read_line(&mut s).unwrap();
    s.trim().parse().unwrap_or(default)
}

fn cmd_history() {
    let h = History::load(HISTORY_FILE);
    for r in h.records.iter().rev().take(20) {
        let ts = &r.timestamp;
        let date = if ts.len() >= 10 { &ts[..10] } else { ts };
        println!("{} | {} | {} | RAM {:.1}MB | {}", r.run_id, r.program, date, r.result_summary.peak_ram_mb, if r.result_summary.success { "OK" } else { "FAIL" });
    }
}

fn cmd_stats() {
    let h = History::load(HISTORY_FILE);
    let total = h.records.len();
    let success = h.records.iter().filter(|r| r.result_summary.success).count();
    let accepted = h.records.iter().filter(|r| matches!(r.human_decision, Some(HumanDecision::Accepted) | Some(HumanDecision::ModifiedAndAccepted { .. }))).count();
    println!("Total runs: {}", total);
    println!("Success rate: {:.0}%", if total>0 { success as f64 / total as f64 * 100.0 } else { 0.0 });
    println!("Accepted recommendations: {}", accepted);
}