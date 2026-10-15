// ai.rs - versiune finală cu acțiune corectă bazată pe reguli
use crate::sandbox::SandboxResult;
use crate::history::{History, AiRecommendation, LearningPatterns};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

#[derive(Serialize)]
struct GroqRequest {
    model: String,
    messages: Vec<GroqMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize)]
struct GroqMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct GroqResponse {
    choices: Vec<GroqChoice>,
}

#[derive(Deserialize)]
struct GroqChoice {
    message: GroqResponseMessage,
}

#[derive(Deserialize)]
struct GroqResponseMessage {
    content: String,
}

pub struct AiAdvisor {
    api_key: Option<String>,
}

impl AiAdvisor {
    pub fn new(_api_key: String) -> Self {
        let key = Self::load_api_key();
        Self { api_key: key }
    }

    fn load_api_key() -> Option<String> {
        if let Ok(key) = env::var("GROQ_API_KEY") {
            if !key.is_empty() {
                eprintln!("[AI] Using API key from environment variable");
                return Some(key);
            }
        }
        if let Ok(content) = std::fs::read_to_string("api_key.txt") {
            let key = content.trim().to_string();
            if !key.is_empty() && key.starts_with("gsk_") {
                eprintln!("[AI] Using API key from api_key.txt");
                return Some(key);
            }
        }
        if let Ok(content) = std::fs::read_to_string(".env") {
            for line in content.lines() {
                if line.starts_with("GROQ_API_KEY=") {
                    let key = line.trim_start_matches("GROQ_API_KEY=").trim().to_string();
                    if !key.is_empty() {
                        eprintln!("[AI] Using API key from .env file");
                        return Some(key);
                    }
                }
            }
        }
        eprintln!("[AI] No API key found.");
        None
    }

    pub fn analyze(&self, result: &SandboxResult, history: &History) -> Result<AiRecommendation, String> {
        let key = self.api_key.as_ref().ok_or("No Groq API key provided. Set GROQ_API_KEY or place key in api_key.txt")?;
        
        let rules = RuleEngine::compute(result);
        
        // Obținem doar reasoning și severity de la LLM; acțiunea o luăm din rules (matematică)
        let (llm_reasoning, llm_severity) = match self.get_llm_reasoning_and_severity_only(result, history, key, &rules) {
            Ok((r, s)) => (r, s),
            Err(e) => {
                eprintln!("[AI] LLM reasoning failed: {}, using rule engine defaults", e);
                (format!("Rule engine: {}; {}; {}", rules.ram_rule, rules.cpu_rule, rules.disk_rule), rules.severity.clone())
            }
        };

        Ok(AiRecommendation {
            suggested_ram_mb: rules.suggested_ram_mb,
            suggested_cpu_time_sec: rules.suggested_cpu_sec,
            suggested_disk_mb: rules.suggested_disk_mb,
            suggested_timeout_sec: rules.suggested_timeout_sec,
            reasoning: llm_reasoning,
            severity: llm_severity,
            action: rules.action.clone(),
        })
    }

    fn get_llm_reasoning_and_severity_only(
        &self,
        result: &SandboxResult,
        history: &History,
        api_key: &str,
        rules: &RuleEngine,
    ) -> Result<(String, String), String> {
        let model = env::var("AI_MODEL").unwrap_or_else(|_| "llama-3.3-70b-versatile".to_string());
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Client build error: {}", e))?;
        
        let historical_context = history.build_ai_context(&result.program);
        
        let system = "\
You are a Linux sandbox resource limit optimizer. Based on the execution data below, output ONLY a JSON object with exactly two fields: reasoning (string, max 150 chars) and severity (low/medium/high). Do NOT include any other fields. Do NOT add markdown or extra text.";

        let anomalies_str = if result.anomalies.is_empty() {
            "none".to_string()
        } else {
            result.anomalies.join(", ")
        };

        let prompt = format!(
            "Program: {program}
Success: {success}
Exit code: {exit_code}
Killed reason: {killed}
RAM: {ram_used:.1}MB / {ram_limit}MB ({ram_pct:.0}%)
CPU: {wall:.2}s / {cpu_limit}s ({cpu_pct:.0}%)
Anomalies: {anomalies}
Historical context: {history}

The rule engine has determined these correct numeric limits (use them to inform your reasoning):
  RAM: {suggested_ram}MB (rule: {ram_rule})
  CPU: {suggested_cpu}s (rule: {cpu_rule})
  Disk: {suggested_disk}MB (rule: {disk_rule})
  Timeout: {suggested_timeout}s

Generate a human-readable reasoning and severity based on this information.",
            program = result.program,
            success = result.success,
            exit_code = result.metrics.exit_code.unwrap_or(-1),
            killed = result.metrics.killed_reason.as_deref().unwrap_or("none"),
            ram_used = result.metrics.peak_ram_mb,
            ram_limit = result.config.ram_mb,
            ram_pct = rules.ram_pct,
            wall = result.metrics.wall_time_sec,
            cpu_limit = result.config.cpu_time_sec,
            cpu_pct = rules.cpu_pct,
            anomalies = anomalies_str,
            suggested_ram = rules.suggested_ram_mb,
            ram_rule = rules.ram_rule,
            suggested_cpu = rules.suggested_cpu_sec,
            cpu_rule = rules.cpu_rule,
            suggested_disk = rules.suggested_disk_mb,
            disk_rule = rules.disk_rule,
            suggested_timeout = rules.suggested_timeout_sec,
            history = historical_context,
        );
        
        let request_body = GroqRequest {
            model,
            messages: vec![
                GroqMessage { role: "system".to_string(), content: system.to_string() },
                GroqMessage { role: "user".to_string(), content: prompt },
            ],
            temperature: 0.1,
            max_tokens: 200,
        };
        
        let response = client
            .post("https://api.groq.com/openai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .map_err(|e| format!("Groq request failed: {}", e))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(format!("Groq API error {}: {}", status, body));
        }
        
        let groq_resp: GroqResponse = response.json().map_err(|e| format!("Failed to parse Groq response: {}", e))?;
        let text = groq_resp.choices[0].message.content.clone();
        
        let clean = text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        
        let json_str = if let Some(start) = clean.find('{') {
            if let Some(end) = clean[start..].rfind('}') {
                &clean[start..start+end+1]
            } else {
                clean
            }
        } else {
            clean
        };
        
        let v: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| format!("Invalid JSON: {} (raw: {})", e, text))?;
        
        let reasoning = v.get("reasoning")
            .and_then(|x| x.as_str())
            .unwrap_or("AI recommendation")
            .to_string();
        let severity = v.get("severity")
            .and_then(|x| x.as_str())
            .unwrap_or(&rules.severity)
            .to_string();
        
        let valid_severity = match severity.as_str() {
            "low" | "medium" | "high" => severity,
            _ => rules.severity.clone(),
        };
        
        Ok((reasoning, valid_severity))
    }
}

struct RuleEngine {
    ram_pct: f64,
    cpu_pct: f64,
    suggested_ram_mb: u64,
    suggested_cpu_sec: u64,
    suggested_disk_mb: u64,
    suggested_timeout_sec: u64,
    ram_rule: &'static str,
    cpu_rule: &'static str,
    disk_rule: &'static str,
    severity: String,
    action: String,
}

impl RuleEngine {
    fn compute(result: &SandboxResult) -> Self {
        let ram_pct = if result.config.ram_mb > 0 {
            (result.metrics.peak_ram_mb / result.config.ram_mb as f64) * 100.0
        } else { 0.0 };
        
        let cpu_pct = if result.config.cpu_time_sec > 0 {
            (result.metrics.wall_time_sec / result.config.cpu_time_sec as f64) * 100.0
        } else { 0.0 };
        
        // Detectare MemoryError explicit în stderr
        let has_memory_error = result.stderr.contains("MemoryError");
        let killed_ram = matches!(&result.metrics.killed_reason, Some(r) if r.contains("memory") || r.contains("signal 9")) || has_memory_error;
        let killed_cpu  = matches!(&result.metrics.killed_reason, Some(r) if r.contains("CPU") || r.contains("signal 24"));
        let killed_disk = matches!(&result.metrics.killed_reason, Some(r) if r.contains("disk") || r.contains("signal 25") || r.contains("SIGXFSZ") || r.contains("File size"));
        let any_kill = killed_ram || killed_cpu || killed_disk;
        
        // RAM rule: creștem DOAR dacă a fost ucis de RAM/MemoryError sau utilizare >80%
        let (suggested_ram_mb, ram_rule) = if killed_ram {
            (result.config.ram_mb * 3, "R1 (killed by RAM or MemoryError)")
        } else if ram_pct > 80.0 {
            (result.config.ram_mb * 2, "R2 (>80% utilization)")
        } else {
            (result.config.ram_mb, "R3 (keep current)")
        };
        
        // CPU rule: nu scăde niciodată dacă procesul a eșuat
        let (suggested_cpu_sec, cpu_rule) = if killed_cpu {
            (result.config.cpu_time_sec * 2, "C1 (killed by CPU)")
        } else if cpu_pct > 80.0 {
            (result.config.cpu_time_sec * 2, "C2 (>80% utilization)")
        } else if cpu_pct >= 20.0 {
            (result.config.cpu_time_sec, "C3 (20-80%, keep)")
        } else {
            if !result.success {
                (result.config.cpu_time_sec, "C4a (failed process, keep)")
            } else {
                let raw = (result.metrics.wall_time_sec * 2.0).ceil() as u64;
                (raw.max(1), "C4 (<20%, tighten)")
            }
        };
        
        // Disk rule
        let (suggested_disk_mb, disk_rule) = if killed_disk {
            (result.config.disk_mb * 2, "D1 (killed by disk)")
        } else {
            (result.config.disk_mb, "D2 (keep)")
        };
        
        // Timeout: la eșec, păstrăm valoarea curentă
        let suggested_timeout_sec = if !result.success {
            result.config.timeout_sec
        } else {
            suggested_cpu_sec + 5
        };
        
        let severity = if any_kill {
            "high".to_string()
        } else if ram_pct > 80.0 || cpu_pct > 80.0 {
            "medium".to_string()
        } else {
            "low".to_string()
        };
        
        let action = if any_kill || ram_pct > 80.0 || cpu_pct > 80.0 {
            "increase_resources".to_string()
        } else if !result.success {
            "keep_current".to_string()
        } else if suggested_ram_mb < result.config.ram_mb || suggested_cpu_sec < result.config.cpu_time_sec {
            "decrease_resources".to_string()
        } else {
            "keep_current".to_string()
        };
        
        Self {
            ram_pct,
            cpu_pct,
            suggested_ram_mb,
            suggested_cpu_sec,
            suggested_disk_mb,
            suggested_timeout_sec,
            ram_rule,
            cpu_rule,
            disk_rule,
            severity,
            action,
        }
    }
}

fn next_power_of_two_min16(n: u64) -> u64 {
    if n <= 16 { return 16; }
    let mut p = 1u64;
    while p < n { p <<= 1; }
    p
}