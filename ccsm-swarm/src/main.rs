use std::sync::Arc;
use tokio::sync::Mutex;

use rmcp::{ErrorData as McpError, schemars, ServiceExt, tool, tool_handler, tool_router};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerInfo, ServerCapabilities};

mod tmux;
mod state;

use state::SwarmState;

const MAX_TEXT_LEN: usize = 65536;
const MAX_TIMEOUT: u64 = 3600;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct TargetParam {
    /// Pane ID (e.g. %0) or label or session:window.pane (optional — lists all if omitted)
    target: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct CaptureParam {
    /// Pane ID (e.g. %0) or label or session:window.pane
    target: String,
    /// Number of lines to capture (-1 = delta from last read, N = last N lines)
    lines: Option<i32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct InjectParam {
    /// Pane ID, label, or session:window.pane
    target: String,
    /// Text to send
    text: String,
    /// Press Enter after typing (default: true)
    #[serde(default = "default_true")]
    enter: Option<bool>,
}

fn default_true() -> Option<bool> { Some(true) }

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct WaitParam {
    /// Pane ID, label, or session:window.pane
    target: String,
    /// String to wait for in pane output
    sentinel: String,
    /// Max seconds to wait (default: 300, max: 3600)
    #[serde(default = "default_300")]
    timeout_secs: u64,
}

fn default_300() -> u64 { 300 }

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct BroadcastParam {
    /// Text to send to all panes
    text: String,
    /// Press Enter after typing (default: true)
    #[serde(default = "default_true")]
    enter: Option<bool>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct LabelParam {
    /// Pane ID or session:window.pane
    target: String,
    /// Label to assign (used for name-based targeting in other tools)
    label: String,
}

#[derive(Clone)]
struct SwarmServer {
    state: Arc<Mutex<SwarmState>>,
    tool_router: ToolRouter<SwarmServer>,
}

impl SwarmServer {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(SwarmState::default())),
            tool_router: Self::tool_router(),
        }
    }

    async fn resolve(&self, target: &str) -> String {
        let state = self.state.lock().await;
        state.resolve_target(target)
    }
}

#[tool_router]
impl SwarmServer {
    /// List all tmux sessions and their panes.
    #[tool(name = "swarm-list-panes", description = "List all tmux panes with session, window, and process info")]
    async fn swarm_list_panes(&self) -> Result<String, McpError> {
        let panes = tmux::list_panes(None).map_err(|e| {
            McpError::internal_error(e.to_string(), None)
        })?;

        let output: Vec<serde_json::Value> = panes.iter().map(|p| {
            serde_json::json!({
                "session": p.session,
                "window": p.window,
                "pane_index": p.pane_index,
                "pane_id": p.pane_id,
                "process": p.process,
            })
        }).collect();

        serde_json::to_string(&output)
            .map_err(|e| McpError::internal_error(format!("json serialization failed: {e}"), None))
    }

    /// Capture pane output. Delta-aware: returns only new content since last read by default.
    /// Omit or pass `lines: -1` for delta mode. Pass `lines: N` (positive) for explicit last N lines (bypasses delta).
    #[tool(name = "swarm-capture", description = "Capture pane output (delta-aware — returns only new content)")]
    async fn swarm_capture(
        &self,
        Parameters(CaptureParam { target, lines }): Parameters<CaptureParam>,
    ) -> Result<String, McpError> {
        let explicit_lines = lines.is_some_and(|n| n > 0);
        let resolved = self.resolve(&target).await;

        let content = match lines {
            Some(n) if n > 0 => tmux::capture_pane(&resolved, Some(n as usize))
                .map_err(|e| McpError::internal_error(e.to_string(), None))?,
            _ => tmux::capture_pane(&resolved, None)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?,
        };

        let total_bytes = content.len();
        let mut state = self.state.lock().await;

        let output = if explicit_lines {
            // Keep byte baseline current even when skipping delta
            state.update_bytes(&resolved, total_bytes);
            content
        } else {
            let delta = state.update_bytes(&resolved, total_bytes);
            if delta == 0 {
                String::new()
            } else if delta >= total_bytes {
                content
            } else {
                content[total_bytes - delta..].to_string()
            }
        };
        drop(state);

        Ok(output)
    }

    /// Send text to a pane. Optionally press Enter after the text.
    #[tool(name = "swarm-inject", description = "Type text into a pane (optionally press Enter)")]
    async fn swarm_inject(
        &self,
        Parameters(InjectParam { target, text, enter }): Parameters<InjectParam>,
    ) -> Result<String, McpError> {
        let enter = enter.unwrap_or(true);
        let resolved = self.resolve(&target).await;

        if text.len() > MAX_TEXT_LEN {
            return Err(McpError::invalid_params(
                format!("text too long ({} bytes, max {MAX_TEXT_LEN})", text.len()),
                None,
            ));
        }

        tmux::send_keys(&resolved, &text, enter)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(serde_json::json!({"ok": true, "target": resolved}).to_string())
    }

    /// Block until a sentinel string appears in pane output.
    /// Polls every 2s. Returns content up to the sentinel on match, or timeout error.
    #[tool(name = "swarm-wait", description = "Block until a sentinel string appears in pane output")]
    async fn swarm_wait(
        &self,
        Parameters(WaitParam { target, sentinel, timeout_secs }): Parameters<WaitParam>,
    ) -> Result<String, McpError> {
        let timeout_secs = timeout_secs.min(MAX_TIMEOUT);
        let resolved = self.resolve(&target).await;
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        loop {
            if start.elapsed() > timeout {
                return Err(McpError::internal_error(
                    format!("wait timed out after {timeout_secs}s, sentinel '{}' not found in pane '{target}'", sentinel),
                    None,
                ));
            }

            let content = tmux::capture_pane(&resolved, Some(300))
                .map_err(|e| McpError::internal_error(
                    format!("wait for sentinel '{}' failed: {e}", sentinel), None,
                ))?;

            if content.contains(&sentinel) {
                let total = content.len();
                let mut state = self.state.lock().await;
                state.update_bytes(&resolved, total);
                drop(state);

                return Ok(serde_json::json!({
                    "ok": true,
                    "content": content,
                }).to_string());
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    /// Consolidated status of all agent panes or a specific one.
    #[tool(name = "swarm-status", description = "Consolidated status of all panes or a specific one")]
    async fn swarm_status(
        &self,
        Parameters(TargetParam { target }): Parameters<TargetParam>,
    ) -> Result<String, McpError> {
        let resolved = match &target {
            Some(t) if !t.is_empty() => Some(self.resolve(t).await),
            _ => None,
        };

        let panes = tmux::list_panes(None)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut results = Vec::new();
        for pane in &panes {
            if let Some(ref r) = resolved
                && pane.pane_id != *r
                && pane.session != *r
            {
                continue;
            }

            let (last_line, error) = match tmux::capture_pane(&pane.pane_id, Some(10)) {
                Ok(content) => {
                    (content.lines().last().unwrap_or("").to_string(), None::<String>)
                }
                Err(e) => {
                    (String::new(), Some(e.to_string()))
                }
            };

            let label = {
                let state = self.state.lock().await;
                state.label_for(&pane.pane_id).map(|s| s.to_string())
            };

            let mut entry = serde_json::json!({
                "session": pane.session,
                "window": pane.window,
                "pane_id": pane.pane_id,
                "process": pane.process,
                "last_line": last_line,
                "label": label,
            });
            if let Some(err) = error {
                entry["error"] = serde_json::Value::String(err);
            }
            results.push(entry);
        }

        Ok(serde_json::json!({"panes": results, "count": results.len()}).to_string())
    }

    /// Send the same text to every pane.
    #[tool(name = "swarm-broadcast", description = "Broadcast text to all panes")]
    async fn swarm_broadcast(
        &self,
        Parameters(BroadcastParam { text, enter }): Parameters<BroadcastParam>,
    ) -> Result<String, McpError> {
        let enter = enter.unwrap_or(true);
        let panes = tmux::list_panes(None)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut results: Vec<serde_json::Value> = Vec::new();
        for pane in &panes {
            match tmux::send_keys(&pane.pane_id, &text, enter) {
                Ok(()) => {
                    results.push(serde_json::json!({
                        "pane_id": pane.pane_id,
                        "ok": true,
                    }));
                }
                Err(e) => {
                    results.push(serde_json::json!({
                        "pane_id": pane.pane_id,
                        "ok": false,
                        "error": e.to_string(),
                    }));
                }
            }
        }

        let ok_count = results.iter().filter(|r| r["ok"].as_bool().unwrap_or(false)).count();
        Ok(serde_json::json!({
            "ok": ok_count > 0,
            "results": results,
            "ok_count": ok_count,
            "fail_count": results.len() - ok_count,
        }).to_string())
    }

    /// Assign a label to a pane for name-based targeting in other tools.
    #[tool(name = "swarm-label", description = "Label a pane for name-based targeting")]
    async fn swarm_label(
        &self,
        Parameters(LabelParam { target, label }): Parameters<LabelParam>,
    ) -> Result<String, McpError> {
        let resolved = self.resolve(&target).await;
        let mut state = self.state.lock().await;
        state.set_label(&resolved, &label);
        Ok(serde_json::json!({"ok": true, "pane_id": resolved, "label": label}).to_string())
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
                version: "0.1.0".into(),
                ..Default::default()
            },
            instructions: None,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tmux::check_tmux()?;

    let server = SwarmServer::new();
    let service = server.serve((tokio::io::stdin(), tokio::io::stdout())).await?;
    service.waiting().await?;
    Ok(())
}
