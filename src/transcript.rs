use anyhow::Result;
use ratatui::text::{Line, Span, Text};
use ratatui::style::{Color, Style};
use serde::Deserialize;
use std::path::PathBuf;

/// A parsed message from a Claude Code JSONL transcript.
#[derive(Debug, Deserialize)]
pub struct RawMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub message: Option<RawMessageBody>,
}

#[derive(Debug, Deserialize)]
pub struct RawMessageBody {
    pub content: Option<Vec<serde_json::Value>>,
}

/// Simplified transcript entry for display.
pub enum TranscriptLine {
    Role(String),
    Text(String),
    ToolCall { name: String, input: String },
    Separator,
}

/// A loaded transcript ready for display.
pub struct TranscriptView {
    pub lines: Vec<TranscriptLine>,
    pub scroll: usize,
    pub session_name: String,
    pub line_count: usize,
}

impl TranscriptView {
    /// Load and parse a JSONL transcript file.
    pub fn load(path: &PathBuf, session_name: &str) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let mut lines: Vec<TranscriptLine> = Vec::new();

        for raw_line in contents.lines() {
            let raw_line = raw_line.trim();
            if raw_line.is_empty() {
                continue;
            }

            let Ok(msg) = serde_json::from_str::<RawMessage>(raw_line) else {
                continue;
            };

            match msg.msg_type.as_str() {
                "user" => {
                    // Extract user message content
                    if let Some(ref message) = msg.message {
                        if let Some(ref content) = message.content {
                            for block in content {
                                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                    if !text.trim().is_empty() {
                                        lines.push(TranscriptLine::Role("You".into()));
                                        lines.push(TranscriptLine::Text(text.to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
                "assistant" => {
                    if let Some(ref message) = msg.message {
                        if let Some(ref content) = message.content {
                            lines.push(TranscriptLine::Role("cds".into()));
                            for block in content {
                                match block.get("type").and_then(|t| t.as_str()) {
                                    Some("text") => {
                                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                            lines.push(TranscriptLine::Text(text.to_string()));
                                        }
                                    }
                                    Some("tool_use") => {
                                        let name = block
                                            .get("name")
                                            .and_then(|n| n.as_str())
                                            .unwrap_or("unknown");
                                        let input = block
                                            .get("input")
                                            .map(|i| serde_json::to_string_pretty(i).unwrap_or_default())
                                            .unwrap_or_default();
                                        lines.push(TranscriptLine::ToolCall {
                                            name: name.to_string(),
                                            input,
                                        });
                                    }
                                    _ => {}
                                }
                            }
                            lines.push(TranscriptLine::Separator);
                        }
                    }
                }
                _ => {}
            }
        }

        let line_count = lines.len();

        Ok(Self {
            lines,
            scroll: 0,
            session_name: session_name.to_string(),
            line_count,
        })
    }

    /// Empty transcript (no file found).
    pub fn empty(session_name: &str) -> Self {
        Self {
            lines: vec![TranscriptLine::Text(
                "No transcript data available for this session.".into(),
            )],
            scroll: 0,
            session_name: session_name.to_string(),
            line_count: 1,
        }
    }

    /// Scroll up by n lines.
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    /// Scroll down by n lines.
    pub fn scroll_down(&mut self, n: usize) {
        let max = self.line_count.saturating_sub(1);
        self.scroll = (self.scroll + n).min(max);
    }

    /// Render the transcript as ratatui Text, starting from `self.scroll`.
    pub fn render(&self, available_height: usize) -> Text<'_> {
        let available = available_height.max(1).saturating_sub(2); // padding
        let start = self.scroll;
        let end = (start + available).min(self.lines.len());

        let slice = &self.lines[start..end];
        let mut lines: Vec<Line> = Vec::new();
        for entry in slice {
            match entry {
                TranscriptLine::Role(name) => {
                    lines.push(Line::from(Span::styled(
                        format!("── {name} ──"),
                        Style::default().fg(Color::Yellow),
                    )));
                }
                TranscriptLine::Text(t) => {
                    for text_line in t.lines() {
                        lines.push(Line::from(Span::styled(
                            text_line.to_string(),
                            Style::default().fg(Color::White),
                        )));
                    }
                }
                TranscriptLine::ToolCall { name, input } => {
                    lines.push(Line::from(Span::styled(
                        format!("  🔧 {name}"),
                        Style::default().fg(Color::Cyan),
                    )));
                    for il in input.lines().take(5) {
                        lines.push(Line::from(Span::styled(
                            format!("    {il}"),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
                TranscriptLine::Separator => {
                    lines.push(Line::from(Span::styled(
                        "─".repeat(40),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
        }

        Text::from(lines)
    }
}

/// Check if a transcript file exists and return its path.
pub fn transcript_path(home: &str, cwd: &str, session_id: &str) -> Option<PathBuf> {
    let cwd_slug = cwd.replace('/', "-");
    let path = PathBuf::from(home)
        .join(".claude")
        .join("projects")
        .join(cwd_slug)
        .join(format!("{session_id}.jsonl"));
    if path.exists() {
        Some(path)
    } else {
        None
    }
}
