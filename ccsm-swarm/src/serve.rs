use std::time::Duration;
use serde::{Deserialize, Serialize};

pub struct ServeClient {
    base: String,
    agent: ureq::Agent,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TokenInfo {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageInfo {
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub tokens: TokenInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageResponse {
    pub info: Option<MessageInfo>,
}

#[derive(Debug, Serialize)]
struct TextPart {
    #[serde(rename = "type")]
    part_type: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct SendMessage {
    parts: Vec<TextPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent: Option<String>,
}

impl ServeClient {
    pub fn new(port: u16) -> Self {
        let config = ureq::config::Config::builder()
            .timeout_global(Some(Duration::from_secs(600)))
            .build();
        Self {
            base: format!("http://127.0.0.1:{}", port),
            agent: ureq::Agent::new_with_config(config),
        }
    }

    pub fn health(&self) -> anyhow::Result<bool> {
        let resp = self.agent.get(format!("{}/health", self.base)).call();
        match resp {
            Ok(r) => Ok(r.status() == 200),
            Err(_) => Ok(false),
        }
    }

    pub fn create_session(&self, title: &str) -> anyhow::Result<Session> {
        let body = serde_json::json!({"title": title});
        let mut resp = self.agent
            .post(format!("{}/session", self.base))
            .header("Content-Type", "application/json")
            .send_json(&body)?;
        let text = resp.body_mut().read_to_string()?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn send_message(&self, session_id: &str, prompt: &str) -> anyhow::Result<MessageInfo> {
        self.send_message_as(session_id, prompt, None)
    }

    pub fn send_message_as(&self, session_id: &str, prompt: &str, agent: Option<&str>) -> anyhow::Result<MessageInfo> {
        let body = SendMessage {
            parts: vec![TextPart {
                part_type: "text".to_string(),
                text: prompt.to_string(),
            }],
            agent: agent.map(|a| a.to_string()),
        };
        let mut resp = self.agent
            .post(format!("{}/session/{}/message", self.base, session_id))
            .header("Content-Type", "application/json")
            .send_json(&body)?;
        let text = resp.body_mut().read_to_string()?;
        let _msg: MessageResponse = serde_json::from_str(&text)?;
        Ok(MessageInfo {
            cost: 0.0,
            tokens: TokenInfo { input: 0, output: 0 },
        })
    }

    pub fn wait_for_session(&self, session_id: &str, timeout_secs: u64) -> anyhow::Result<MessageInfo> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);
        loop {
            if start.elapsed() > timeout {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(2));

            if let Ok(mut status_resp) = self.agent
                .get(format!("{}/session/status", self.base))
                .call() {
                let status_text = status_resp.body_mut().read_to_string().unwrap_or_default();
                if let Ok(status_map) = serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(&status_text) {
                    if let Some(entry) = status_map.get(session_id) {
                        if let Some(typ) = entry.get("type").and_then(|v| v.as_str()) {
                            if typ != "busy" {
                                return self.session_tokens(session_id);
                            }
                        }
                    } else {
                        return self.session_tokens(session_id);
                    }
                }
            }
        }
        self.session_tokens(session_id)
    }

    fn session_tokens(&self, session_id: &str) -> anyhow::Result<MessageInfo> {
        let mut resp = self.agent
            .get(format!("{}/session/{}", self.base, session_id))
            .call()?;
        let text = resp.body_mut().read_to_string()?;
        let session: serde_json::Value = serde_json::from_str(&text)?;
        let tokens = session.get("tokens").cloned().unwrap_or(serde_json::json!({"input": 0, "output": 0}));
        Ok(MessageInfo {
            cost: session.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0),
            tokens: TokenInfo {
                input: tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0),
                output: tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0),
            },
        })
    }

    pub fn delete_session(&self, session_id: &str) -> anyhow::Result<()> {
        self.agent
            .delete(format!("{}/session/{}", self.base, session_id))
            .call()?;
        Ok(())
    }
}

pub fn find_available_port() -> anyhow::Result<u16> {
    use std::net::TcpListener;
    let l = TcpListener::bind("127.0.0.1:0")?;
    let port = l.local_addr()?.port();
    drop(l);
    Ok(port)
}
