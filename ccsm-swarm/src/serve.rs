use std::time::Duration;
use serde::{Deserialize, Serialize};

pub struct ServeClient {
    base: String,
    agent: ureq::Agent,
    auth_header: Option<String>,
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
    #[serde(default)]
    pub reasoning: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionData {
    pub data: SessionDetail,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionDetail {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub tokens: TokenInfo,
    #[serde(default)]
    pub cost: f64,
}

fn load_auth() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let config_dir = std::path::PathBuf::from(&home).join(".config").join("opencode");
    let entries = std::fs::read_dir(&config_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("service-") && name.ends_with(".json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(pw) = val.get("password").and_then(|v| v.as_str()) {
                            let encoded = b64_encode(&format!("opencode:{pw}"));
                            return Some(format!("Basic {encoded}"));
                        }
                    }
                }
            }
        }
    }
    None
}

fn b64_encode(input: &str) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i] as u32;
        let b1 = bytes.get(i + 1).copied().unwrap_or(0) as u32;
        let b2 = bytes.get(i + 2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < bytes.len() {
            out.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

fn apply_auth<B>(req: ureq::RequestBuilder<B>, auth: Option<&str>) -> ureq::RequestBuilder<B> {
    match auth {
        Some(val) => req.header("Authorization", val),
        None => req,
    }
}

impl ServeClient {
    pub fn connect() -> anyhow::Result<Self> {
        let port: u16 = std::env::var("CCSM_SERVE_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4096);

        let config = ureq::config::Config::builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let auth_header = load_auth();
        let base = format!("http://127.0.0.1:{port}");

        let mut req = agent.get(format!("{base}/api/health"));
        if let Some(ref auth) = auth_header {
            req = req.header("Authorization", auth.as_str());
        }
        match req.call() {
            Ok(r) if r.status() == 200 => {}
            Ok(r) => anyhow::bail!("opencode2 serve returned {status} on /api/health", status = r.status()),
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("Connection refused") || msg.contains("connection refused") {
                    anyhow::bail!("opencode2 daemon not running (port {port})");
                }
                anyhow::bail!("opencode2 serve health check failed: {e}");
            }
        }

        Ok(Self { base, agent, auth_header })
    }

    fn auth(&self) -> Option<&str> {
        self.auth_header.as_deref()
    }

    pub fn health(&self) -> bool {
        let req = apply_auth(self.agent.get(format!("{}/api/health", self.base)), self.auth());
        req.call().is_ok_and(|r| r.status() == 200)
    }

    pub fn create_session(&self, title: &str) -> anyhow::Result<Session> {
        let body = serde_json::json!({"title": title});
        let mut resp = apply_auth(
            self.agent.post(format!("{}/api/session", self.base))
                .header("Content-Type", "application/json"),
            self.auth(),
        ).send_json(&body)?;
        let text = resp.body_mut().read_to_string()?;
        let data: SessionData = serde_json::from_str(&text)?;
        Ok(Session { id: data.data.id, title: data.data.title })
    }

    pub fn get_session(&self, id: &str) -> anyhow::Result<SessionDetail> {
        let mut resp = apply_auth(
            self.agent.get(format!("{}/api/session/{id}", self.base)),
            self.auth(),
        ).call()?;
        let text = resp.body_mut().read_to_string()?;
        let data: SessionData = serde_json::from_str(&text)?;
        Ok(data.data)
    }

    pub fn send_prompt(&self, session_id: &str, text: &str) -> anyhow::Result<()> {
        let body = serde_json::json!({"text": text});
        let mut req = self.agent
            .post(format!("{}/api/session/{session_id}/prompt", self.base))
            .header("Content-Type", "application/json");
        if let Some(auth) = self.auth() {
            req = req.header("Authorization", auth);
        }
        req.send_json(&body)?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> anyhow::Result<()> {
        let resp = apply_auth(
            self.agent.delete(format!("{}/api/session/{id}", self.base)),
            self.auth(),
        ).call()?;
        let status = resp.status();
        if status != 200 {
            anyhow::bail!("DELETE /api/session/{id} returned {status}");
        }
        Ok(())
    }
}
