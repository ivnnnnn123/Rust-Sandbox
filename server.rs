mod sandbox;
mod history;
mod ai;

use sandbox::{Sandbox, SandboxConfig};
use history::{History, HumanDecision, AiRecommendation};
use ai::AiAdvisor;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tiny_http::{Server, Response, Header};

const HISTORY_FILE: &str = "history.json";

#[derive(Deserialize)]
struct RunRequest {
    program: String,
    args: Option<Vec<String>>,
    config: Option<SandboxConfig>,
}

#[derive(Serialize)]
struct RunResponse {
    result: sandbox::SandboxResult,
    recommendation: Option<AiRecommendation>,
    history_count: usize,
}

#[derive(Deserialize)]
struct DecisionRequest {
    run_id: String,
    decision: String,
    new_config: Option<SandboxConfig>,
}

type SharedHistory = Arc<Mutex<History>>;
type PendingRun = Arc<Mutex<Option<(sandbox::SandboxResult, Option<AiRecommendation>, SandboxConfig)>>>;

fn cors_headers() -> Vec<Header> {
    vec![
        Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
        Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
        Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap(),
        Header::from_bytes("Content-Type", "application/json").unwrap(),
    ]
}

fn json_resp(body: String, status: u16) -> Response<std::io::Cursor<Vec<u8>>> {
    let data = body.into_bytes();
    let len = data.len();
    Response::new(tiny_http::StatusCode(status), cors_headers(), std::io::Cursor::new(data), Some(len), None)
}

fn ok_json<T: Serialize>(data: T) -> String {
    serde_json::json!({ "success": true, "data": data }).to_string()
}

fn err_json(msg: &str) -> String {
    serde_json::json!({ "success": false, "error": msg }).to_string()
}

fn main() {
    let port = "7878";
    let addr = format!("0.0.0.0:{}", port);
    let history: SharedHistory = Arc::new(Mutex::new(History::load(HISTORY_FILE)));
    let pending: PendingRun = Arc::new(Mutex::new(None));

    let server = Server::http(&addr).expect("Failed to bind");
    println!("Server running on http://{}", addr);

    for mut request in server.incoming_requests() {
        let method = request.method().as_str().to_string();
        let url = request.url().to_string();
        let path = url.split('?').next().unwrap_or("/");

        let (body, status) = if method == "OPTIONS" {
            ("{}".to_string(), 200)
        } else {
            match (method.as_str(), path) {
                ("GET", "/api/history") => {
                    let h = history.lock().unwrap();
                    (ok_json(&h.records), 200)
                }
                ("GET", "/api/stats") => {
                    let h = history.lock().unwrap();
                    let total = h.records.len();
                    let success = h.records.iter().filter(|r| r.result_summary.success).count();
                    let accepted = h.records.iter().filter(|r| matches!(r.human_decision, Some(HumanDecision::Accepted) | Some(HumanDecision::ModifiedAndAccepted { .. }))).count();
                    let stats = serde_json::json!({
                        "total_runs": total,
                        "success_rate": if total>0 { success as f64 / total as f64 * 100.0 } else { 0.0 },
                        "acceptance_rate": if total>0 { accepted as f64 / total as f64 * 100.0 } else { 0.0 }
                    });
                    (ok_json(stats), 200)
                }
                // NEW: endpoint to expose learning patterns for a program
                ("GET", p) if p.starts_with("/api/learning/") => {
                    let program = &p["/api/learning/".len()..];
                    let program = urlencoding_decode(program);
                    let h = history.lock().unwrap();
                    match h.get_learning_summary(&program) {
                        Some(pat) => (ok_json(pat), 200),
                        None => (ok_json(serde_json::json!({ "message": "No learning data yet for this program" })), 200),
                    }
                }
                ("POST", "/api/run") => {
                    let mut body_str = String::new();
                    let _ = request.as_reader().read_to_string(&mut body_str);
                    match serde_json::from_str::<RunRequest>(&body_str) {
                        Err(e) => (err_json(&format!("Bad request: {}", e)), 400),
                        Ok(req) => {
                            let base_config = {
                                let h = history.lock().unwrap();
                                req.config.clone().or_else(|| h.suggest_baseline_config(&req.program)).unwrap_or_default()
                            };

                            // Security flags: if user sent a config, use their security settings;
                            // otherwise fall back to the baseline
                            let effective_config = if let Some(ref user_cfg) = req.config {
                                // Merge: take resource limits from user, but keep their security flags
                                SandboxConfig {
                                    allow_network: user_cfg.allow_network,
                                    allow_file_write: user_cfg.allow_file_write,
                                    ..base_config.clone()
                                }
                            } else {
                                base_config.clone()
                            };

                            let sandbox = Sandbox::new(effective_config.clone());
                            let args: Vec<&str> = req.args.as_deref().unwrap_or(&[]).iter().map(|s| s.as_str()).collect();
                            match sandbox.run(&req.program, &args) {
                                Err(e) => (err_json(&e), 500),
                                Ok(result) => {
                                    let advisor = AiAdvisor::new("".to_string());
                                    let recommendation = match advisor.analyze(&result, &history.lock().unwrap()) {
                                        Ok(rec) => Some(rec),
                                        Err(e) => {
                                            eprintln!("[AI] Groq API error in server: {}", e);
                                            None
                                        }
                                    };
                                    let count = history.lock().unwrap().records.len();
                                    *pending.lock().unwrap() = Some((result.clone(), recommendation.clone(), effective_config));
                                    (ok_json(RunResponse { result, recommendation, history_count: count }), 200)
                                }
                            }
                        }
                    }
                }
                ("POST", "/api/decision") => {
                    let mut body_str = String::new();
                    let _ = request.as_reader().read_to_string(&mut body_str);
                    match serde_json::from_str::<DecisionRequest>(&body_str) {
                        Err(e) => (err_json(&format!("Bad request: {}", e)), 400),
                        Ok(req) => {
                            let mut pend = pending.lock().unwrap();
                            if let Some((result, rec, base_config)) = pend.take() {
                                let (decision, effective) = match req.decision.as_str() {
                                    "accept" => {
                                        let new_cfg = rec.as_ref().map(|r| SandboxConfig {
                                            ram_mb: r.suggested_ram_mb,
                                            cpu_time_sec: r.suggested_cpu_time_sec,
                                            disk_mb: r.suggested_disk_mb,
                                            timeout_sec: r.suggested_timeout_sec,
                                            // Preserve security flags from the run
                                            allow_network: base_config.allow_network,
                                            allow_file_write: base_config.allow_file_write,
                                            ..base_config.clone()
                                        }).unwrap_or(base_config.clone());
                                        (Some(HumanDecision::Accepted), new_cfg)
                                    }
                                    "reject" => (Some(HumanDecision::Rejected), base_config),
                                    "modify" => {
                                        let cfg = req.new_config.unwrap_or(base_config);
                                        (Some(HumanDecision::ModifiedAndAccepted { new_config: cfg.clone() }), cfg)
                                    }
                                    _ => (None, base_config),
                                };
                                let mut h = history.lock().unwrap();
                                h.add_run(&result, rec, decision, effective.clone());
                                let _ = h.save(HISTORY_FILE);

                                // Return the effective config so the UI can update sliders immediately
                                (ok_json(serde_json::json!({
                                    "saved": true,
                                    "effective_config": effective
                                })), 200)
                            } else {
                                (err_json("No pending run"), 400)
                            }
                        }
                    }
                }
                _ => (err_json("Not found"), 404),
            }
        };
        let _ = request.respond(json_resp(body, status));
    }
}

/// Minimal URL percent-decode (handles %2F etc.)
fn urlencoding_decode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    let bytes: Vec<u8> = s.bytes().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i+1..i+3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    result.push(byte as char);
                    i += 3;
                    continue;
                }
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    let _ = chars; // suppress warning
    result
}