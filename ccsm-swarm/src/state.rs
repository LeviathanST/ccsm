use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SwarmRun {
    pub id: String,
    pub goals: Vec<String>,
    pub timeout_secs: u64,
    pub workers: Vec<Worker>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Worker {
    pub name: String,
    pub pid: u32,
    pub status: String,
    pub goal: String,
}

#[derive(Debug, Serialize)]
pub struct RunSummary {
    pub id: String,
    pub status: String,
    pub goals: Vec<String>,
    pub workers: Vec<WorkerSummary>,
}

#[derive(Debug, Serialize)]
pub struct WorkerSummary {
    pub name: String,
    pub status: String,
    pub pid: u32,
}

pub struct SwarmState {
    runs: HashMap<String, SwarmRun>,
    run_order: Vec<String>,
    swarm_dir: PathBuf,
}

impl Default for SwarmState {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        Self {
            runs: HashMap::new(),
            run_order: Vec::new(),
            swarm_dir: PathBuf::from(home).join(".ccsm").join("swarm"),
        }
    }
}

impl SwarmState {
    fn pid_alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    pub fn create_run(&mut self, goals: &[String], timeout_secs: u64) -> String {
        let id = format!("run-{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
        let run = SwarmRun {
            id: id.clone(),
            goals: goals.to_vec(),
            timeout_secs,
            workers: Vec::new(),
        };
        let _ = std::fs::create_dir_all(self.swarm_dir.join(&id));
        self.runs.insert(id.clone(), run);
        self.run_order.push(id.clone());
        id
    }

    pub fn add_worker(&mut self, run_id: &str, name: &str, pid: u32, goal: String) {
        if let Some(run) = self.runs.get_mut(run_id) {
            run.workers.push(Worker { name: name.to_string(), pid, status: "running".to_string(), goal });
        }
    }

    pub fn run_summary(&self, run_id: &str) -> Option<RunSummary> {
        self.runs.get(run_id).map(|r| {
            let all_done = r.workers.iter().all(|w| w.status == "done" || w.status == "failed");
            let status = if all_done { "done" } else { "running" };
            RunSummary {
                id: r.id.clone(),
                status: status.to_string(),
                goals: r.goals.clone(),
                workers: r.workers.iter().map(|w| {
                    let status = if w.status == "running" && !Self::pid_alive(w.pid) {
                        "done"
                    } else {
                        &w.status
                    };
                    WorkerSummary { name: w.name.clone(), status: status.to_string(), pid: w.pid }
                }).collect(),
            }
        })
    }

    pub fn latest_run_summary(&self) -> Option<RunSummary> {
        self.run_order.last().and_then(|id| self.run_summary(id))
    }

    pub fn kill_run(&mut self, run_id: Option<&str>) -> bool {
        let target = match run_id {
            Some(id) => id.to_string(),
            None => self.run_order.last().cloned().unwrap_or_default(),
        };
        if let Some(run) = self.runs.get_mut(&target) {
            for w in &run.workers {
                if Self::pid_alive(w.pid) {
                    unsafe { libc::kill(w.pid as i32, libc::SIGTERM); }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    unsafe { libc::kill(w.pid as i32, libc::SIGKILL); }
                }
            }
            true
        } else {
            false
        }
    }

    pub fn save_meta(&self, run_id: &str) {
        if let Some(run) = self.runs.get(run_id) {
            let path = self.swarm_dir.join(run_id).join("meta.json");
            if let Ok(json) = serde_json::to_string_pretty(run) {
                let _ = std::fs::write(&path, json);
            }
        }
    }
}
