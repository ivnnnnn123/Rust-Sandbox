use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use chrono::Utc;
use crate::sandbox::{SandboxConfig, SandboxResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HumanDecision {
    Accepted,
    Rejected,
    ModifiedAndAccepted { new_config: SandboxConfig },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRecommendation {
    pub suggested_ram_mb: u64,
    pub suggested_cpu_time_sec: u64,
    pub suggested_disk_mb: u64,
    pub suggested_timeout_sec: u64,
    pub reasoning: String,
    pub severity: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub run_id: String,
    pub program: String,
    pub timestamp: String,
    pub result_summary: RunSummary,
    pub ai_recommendation: Option<AiRecommendation>,
    pub human_decision: Option<HumanDecision>,
    pub effective_config: SandboxConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub success: bool,
    pub peak_ram_mb: f64,
    pub wall_time_sec: f64,
    pub exit_code: Option<i32>,
    pub killed_reason: Option<String>,
    pub anomaly_count: usize,
    pub anomalies: Vec<String>,
}

/// Patterns learned from human corrections — used to bias future AI prompts
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LearningPatterns {
    /// How many times the human overrode the AI's RAM suggestion (and by how much on average)
    pub ram_correction_count: u64,
    pub ram_correction_avg_delta: f64,   // positive = human prefers higher than AI suggested
    /// Same for CPU
    pub cpu_correction_count: u64,
    pub cpu_correction_avg_delta: f64,
    /// Acceptance rate per program
    pub accept_count: u64,
    pub reject_count: u64,
    pub modify_count: u64,
    /// Average actual RAM usage at time of "correct" runs
    pub avg_successful_ram_mb: f64,
    pub successful_run_count: u64,
    /// Last N human-preferred configs (most recent first)
    pub preferred_configs: Vec<SandboxConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    pub records: Vec<RunRecord>,
    /// Per-program learning patterns
    pub learning: std::collections::HashMap<String, LearningPatterns>,
}

impl History {
    pub fn load(path: &str) -> Self {
        if Path::new(path).exists() {
            if let Ok(data) = fs::read_to_string(path) {
                if let Ok(h) = serde_json::from_str::<History>(&data) {
                    return h;
                }
                // Backwards compat: old format without learning field
                if let Ok(old) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(records_val) = old.get("records") {
                        if let Ok(records) = serde_json::from_value::<Vec<RunRecord>>(records_val.clone()) {
                            return Self { records, learning: Default::default() };
                        }
                    }
                }
            }
        }
        Self { records: Vec::new(), learning: Default::default() }
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, data).map_err(|e| e.to_string())
    }

    pub fn add_run(
        &mut self,
        result: &SandboxResult,
        rec: Option<AiRecommendation>,
        decision: Option<HumanDecision>,
        effective: SandboxConfig,
    ) {
        // Update learning patterns before inserting
        self.update_learning(&result.program, result, &rec, &decision, &effective);

        self.records.push(RunRecord {
            run_id: result.run_id.clone(),
            program: result.program.clone(),
            timestamp: Utc::now().to_rfc3339(),
            result_summary: RunSummary {
                success: result.success,
                peak_ram_mb: result.metrics.peak_ram_mb,
                wall_time_sec: result.metrics.wall_time_sec,
                exit_code: result.metrics.exit_code,
                killed_reason: result.metrics.killed_reason.clone(),
                anomaly_count: result.anomalies.len(),
                anomalies: result.anomalies.clone(),
            },
            ai_recommendation: rec,
            human_decision: decision,
            effective_config: effective,
        });
    }

    fn update_learning(
        &mut self,
        program: &str,
        result: &SandboxResult,
        rec: &Option<AiRecommendation>,
        decision: &Option<HumanDecision>,
        effective: &SandboxConfig,
    ) {
        let pat = self.learning.entry(program.to_string()).or_default();

        match decision {
            Some(HumanDecision::Accepted) => {
                pat.accept_count += 1;
            }
            Some(HumanDecision::Rejected) => {
                pat.reject_count += 1;
            }
            Some(HumanDecision::ModifiedAndAccepted { new_config }) => {
                pat.modify_count += 1;
                // Learn the delta between AI suggestion and human correction
                if let Some(r) = rec {
                    let ram_delta = new_config.ram_mb as f64 - r.suggested_ram_mb as f64;
                    let cpu_delta = new_config.cpu_time_sec as f64 - r.suggested_cpu_time_sec as f64;
                    // Running average
                    pat.ram_correction_avg_delta = (pat.ram_correction_avg_delta * pat.ram_correction_count as f64 + ram_delta)
                        / (pat.ram_correction_count + 1) as f64;
                    pat.ram_correction_count += 1;
                    pat.cpu_correction_avg_delta = (pat.cpu_correction_avg_delta * pat.cpu_correction_count as f64 + cpu_delta)
                        / (pat.cpu_correction_count + 1) as f64;
                    pat.cpu_correction_count += 1;
                }
                // Store preferred configs (keep last 5)
                pat.preferred_configs.insert(0, new_config.clone());
                pat.preferred_configs.truncate(5);
            }
            None => {}
        }

        // Track successful RAM usage
        if result.success {
            pat.avg_successful_ram_mb = (pat.avg_successful_ram_mb * pat.successful_run_count as f64 + result.metrics.peak_ram_mb)
                / (pat.successful_run_count + 1) as f64;
            pat.successful_run_count += 1;
        }

        // Also store accepted configs as preferred
        if matches!(decision, Some(HumanDecision::Accepted)) {
            pat.preferred_configs.insert(0, effective.clone());
            pat.preferred_configs.truncate(5);
        }
    }

    pub fn get_program_history(&self, program: &str) -> Vec<&RunRecord> {
        self.records.iter().filter(|r| r.program == program).collect()
    }

    pub fn build_ai_context(&self, program: &str) -> String {
        let hist = self.get_program_history(program);
        let pat = self.learning.get(program);

        let mut ctx = String::new();

        if hist.is_empty() {
            ctx.push_str(&format!("No previous runs for '{}'.\n", program));
        } else {
            let total = hist.len();
            let successes = hist.iter().filter(|r| r.result_summary.success).count();
            ctx.push_str(&format!("History for '{}': {} runs ({} success).\n", program, total, successes));
            ctx.push_str("Last 3 runs:\n");
            for rec in hist.iter().rev().take(3) {
                ctx.push_str(&format!(
                    "  - RAM: {:.1}MB (limit {}MB), Time: {:.2}s (limit {}s), Success: {}, Decision: {}\n",
                    rec.result_summary.peak_ram_mb,
                    rec.effective_config.ram_mb,
                    rec.result_summary.wall_time_sec,
                    rec.effective_config.cpu_time_sec,
                    rec.result_summary.success,
                    match &rec.human_decision {
                        Some(HumanDecision::Accepted) => "accepted".to_string(),
                        Some(HumanDecision::Rejected) => "rejected".to_string(),
                        Some(HumanDecision::ModifiedAndAccepted { .. }) => "modified".to_string(),
                        None => "none".to_string(),
                    }
                ));
            }
        }

        // Inject learning patterns into the AI context
        if let Some(pat) = pat {
            ctx.push_str("\nLEARNED HUMAN PREFERENCES (use these to calibrate your recommendation):\n");

            let total_decisions = pat.accept_count + pat.reject_count + pat.modify_count;
            if total_decisions > 0 {
                let accept_rate = pat.accept_count as f64 / total_decisions as f64 * 100.0;
                ctx.push_str(&format!(
                    "  Acceptance rate: {:.0}% ({} accepted, {} rejected, {} modified)\n",
                    accept_rate, pat.accept_count, pat.reject_count, pat.modify_count
                ));
            }

            if pat.ram_correction_count > 0 {
                let direction = if pat.ram_correction_avg_delta > 0.0 { "higher" } else { "lower" };
                ctx.push_str(&format!(
                    "  RAM corrections: human consistently sets RAM {:.0}MB {} than AI suggests (based on {} corrections)\n",
                    pat.ram_correction_avg_delta.abs(), direction, pat.ram_correction_count
                ));
                ctx.push_str(&format!(
                    "  → ADJUST your RAM suggestion by {:.0}MB {:}\n",
                    pat.ram_correction_avg_delta.abs(), direction
                ));
            }

            if pat.cpu_correction_count > 0 {
                let direction = if pat.cpu_correction_avg_delta > 0.0 { "higher" } else { "lower" };
                ctx.push_str(&format!(
                    "  CPU corrections: human sets CPU {}s {} than AI suggests (based on {} corrections)\n",
                    pat.cpu_correction_avg_delta.abs().round() as i64, direction, pat.cpu_correction_count
                ));
                ctx.push_str(&format!(
                    "  → ADJUST your CPU suggestion by {}s {:}\n",
                    pat.cpu_correction_avg_delta.abs().round() as i64, direction
                ));
            }

            if pat.successful_run_count > 0 {
                ctx.push_str(&format!(
                    "  Average actual RAM in successful runs: {:.1}MB\n",
                    pat.avg_successful_ram_mb
                ));
            }

            if let Some(pref) = pat.preferred_configs.first() {
                ctx.push_str(&format!(
                    "  Last human-approved config: RAM={}MB, CPU={}s, Disk={}MB, Timeout={}s\n",
                    pref.ram_mb, pref.cpu_time_sec, pref.disk_mb, pref.timeout_sec
                ));
            }

            if pat.reject_count > pat.accept_count && total_decisions > 2 {
                ctx.push_str("  WARNING: Human rejects AI recommendations more often than accepts — be more conservative and closer to preferred configs.\n");
            }
        }

        ctx
    }

    pub fn get_learning_summary(&self, program: &str) -> Option<&LearningPatterns> {
        self.learning.get(program)
    }

    pub fn suggest_baseline_config(&self, program: &str) -> Option<SandboxConfig> {
        // Prefer learned preferred configs first
        if let Some(pat) = self.learning.get(program) {
            if let Some(cfg) = pat.preferred_configs.first() {
                return Some(cfg.clone());
            }
        }
        // Fallback to last accepted record
        let accepted: Vec<_> = self.get_program_history(program).iter()
            .filter(|r| matches!(r.human_decision, Some(HumanDecision::Accepted) | Some(HumanDecision::ModifiedAndAccepted { .. })))
            .map(|r| r.effective_config.clone())
            .collect();
        accepted.into_iter().last()
    }
}