use std::time::Duration;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ServeClient {
    base: String,
    agent: ureq::Agent,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub directory: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TokenInfo {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
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
pub struct TextPart {
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct SendMessage {
    pub parts: Vec<TextPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SessionStatusEntry {
    #[serde(rename = "type")]
    pub status_type: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionStatus {
    #[serde(flatten)]
    pub entries: std::collections::HashMap<String, SessionStatusEntry>,
}

#[derive(Debug, Serialize)]
pub struct CreateSession {
    pub title: String,
}

impl ServeClient {
    pub fn new(port: u16) -> Result<Self> {
        let config = ureq::config::Config::builder()
            .timeout_global(Some(Duration::from_secs(600)))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        Ok(Self {
            base: format!("http://127.0.0.1:{}", port),
            agent,
        })
    }

    pub fn health(&self) -> Result<bool> {
        let resp = self.agent.get(format!("{}/health", self.base)).call();
        match resp {
            Ok(r) => Ok(r.status() == 200),
            Err(_) => Ok(false),
        }
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let mut resp = self.agent
            .get(format!("{}/session", self.base))
            .call()
            .context("GET /session failed")?;
        let body = resp.body_mut();
        let text = body.read_to_string().context("failed to read response")?;
        serde_json::from_str(&text).context("failed to parse session list")
    }

    pub fn create_session(&self, req: CreateSession) -> Result<Session> {
        let mut resp = self.agent
            .post(format!("{}/session", self.base))
            .header("Content-Type", "application/json")
            .send_json(&req)
            .context("POST /session failed")?;
        let body = resp.body_mut();
        let text = body.read_to_string().context("failed to read response")?;
        serde_json::from_str(&text).context("failed to parse created session")
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.agent
            .delete(format!("{}/session/{}", self.base, id))
            .call()
            .context("DELETE /session/:id failed")?;
        Ok(())
    }

    pub fn send_message(&self, session_id: &str, prompt: &str) -> Result<MessageInfo> {
        self.send_message_as(session_id, prompt, None)
    }

    pub fn send_message_as(&self, session_id: &str, prompt: &str, agent: Option<&str>) -> Result<MessageInfo> {
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
            .send_json(&body)
            .context("POST /session/:id/message failed")?;
        let text = resp.body_mut().read_to_string().context("failed to read message response")?;
        let msg: MessageResponse = serde_json::from_str(&text)
            .context("failed to parse message response")?;
        Ok(msg.info.unwrap_or(MessageInfo {
            cost: 0.0,
            tokens: TokenInfo { input: 0, output: 0, reasoning: 0 },
        }))
    }

    pub fn active_sessions(&self) -> Result<Vec<String>> {
        let mut resp = self.agent
            .get(format!("{}/session/status", self.base))
            .call()
            .context("GET /session/status failed")?;
        let body = resp.body_mut();
        let text = body.read_to_string().context("failed to read response")?;
        let status: SessionStatus = serde_json::from_str(&text)
            .context("failed to parse session status")?;
        Ok(status.entries.keys().cloned().collect())
    }
}
