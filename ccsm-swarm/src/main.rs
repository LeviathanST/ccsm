use std::sync::Arc;
use tokio::sync::Mutex;

use rmcp::{ErrorData as McpError, schemars, ServiceExt, tool, tool_handler, tool_router};
use rmcp::model::{ServerInfo, ServerCapabilities};

fn workspace_path() -> std::path::PathBuf {
    if let Ok(ws) = std::env::var("CCSM_WORKSPACE") {
        return std::path::PathBuf::from(ws);
    }
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"))
}
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;

mod state;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SpawnParam {
    goals: Vec<String>,
    #[serde(default = "default_600")]
    timeout_secs: u64,
}

fn default_600() -> u64 { 600 }

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct StatusParam {
    run_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct KillParam {
    run_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct WaitParam {
    run_id: Option<String>,
    #[serde(default = "default_600")]
    timeout_secs: u64,
}

#[derive(Clone)]
struct SwarmServer {
    state: Arc<Mutex<state::SwarmState>>,
    tool_router: ToolRouter<SwarmServer>,
}

impl SwarmServer {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(state::SwarmState::default())),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl SwarmServer {
    #[tool(name = "swarm-spawn", description = "Spawn worker sessions via opencode2 run")]
    async fn swarm_spawn(
        &self,
        Parameters(SpawnParam { goals, timeout_secs }): Parameters<SpawnParam>,
    ) -> Result<String, McpError> {
        if goals.is_empty() {
            return Err(McpError::invalid_params("at least one goal required", None));
        }

        let mut state = self.state.lock().await;
        let run_id = state.create_run(&goals, timeout_secs);

        let mut results = Vec::new();
        for (i, goal) in goals.iter().enumerate() {
            let name = format!("worker-{}", i + 1);
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            let out_path = std::path::PathBuf::from(&home)
                .join(".ccsm").join("swarm").join(&run_id)
                .join(format!("{}.jsonl", name));

            // Create output directory
            if let Some(parent) = out_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let out_file = match std::fs::File::create(&out_path) {
                Ok(f) => f,
                Err(e) => {
                    results.push(serde_json::json!({"name": name, "goal": goal, "status": "error", "error": format!("create output: {e}")}));
                    continue;
                }
            };

            let mut cmd = std::process::Command::new("opencode2");
            cmd.args(["run", "--format", "json", "--auto", "--agent", "build", "--title", &name, goal]);
            cmd.current_dir(workspace_path());
            cmd.stdout(out_file);
            cmd.stderr(std::process::Stdio::null());

            match cmd.spawn() {
                Ok(child) => {
                    let pid = child.id();
                    std::mem::forget(child); // Detach
                    state.add_worker(&run_id, &name, pid, goal.clone());
                    results.push(serde_json::json!({"name": name, "pid": pid, "goal": goal, "status": "spawned"}));
                }
                Err(e) => {
                    results.push(serde_json::json!({"name": name, "goal": goal, "status": "error", "error": e.to_string()}));
                }
            }
        }

        state.save_meta(&run_id);
        let summary = state.run_summary(&run_id);
        Ok(serde_json::to_string(&summary).unwrap_or_default())
    }

    #[tool(name = "swarm-status", description = "Check status of swarm workers")]
    async fn swarm_status(
        &self,
        Parameters(StatusParam { run_id }): Parameters<StatusParam>,
    ) -> Result<String, McpError> {
        let state = self.state.lock().await;
        let summary = match run_id {
            Some(id) => state.run_summary(&id),
            None => state.latest_run_summary(),
        };
        match summary {
            Some(s) => Ok(serde_json::to_string(&s).unwrap_or_default()),
            None => Err(McpError::internal_error("no swarm run found", None)),
        }
    }

    #[tool(name = "swarm-kill", description = "Kill active swarm run")]
    async fn swarm_kill(
        &self,
        Parameters(KillParam { run_id }): Parameters<KillParam>,
    ) -> Result<String, McpError> {
        let mut state = self.state.lock().await;
        let killed = state.kill_run(run_id.as_deref());
        if killed {
            Ok(serde_json::json!({"ok": true}).to_string())
        } else {
            Err(McpError::internal_error("no active swarm run to kill", None))
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl rmcp::handler::server::ServerHandler for SwarmServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: rmcp::model::Implementation {
                name: "ccsm-swarm".into(),
                version: "0.3.0".into(),
                ..Default::default()
            },
            instructions: None,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = SwarmServer::new();
    let service = server.serve((tokio::io::stdin(), tokio::io::stdout())).await?;
    service.waiting().await?;
    Ok(())
}
