//! Integration test helpers for ccsm CLI.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Once;

static BUILD: Once = Once::new();

/// Ensure the ccsm binary is built before any integration test runs.
pub fn ensure_built() {
    BUILD.call_once(|| {
        let status = Command::new("cargo")
            .args(["build", "--release", "--quiet"])
            .status()
            .expect("cargo build failed");
        assert!(status.success(), "cargo build --release failed");
    });
}

/// Path to the ccsm binary.
pub fn ccsm_binary() -> PathBuf {
    // CARGO_BIN_EXE_ccsm is set by cargo test for integration tests
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_ccsm") {
        return PathBuf::from(path);
    }
    // Fallback: look relative to the test binary
    PathBuf::from("target/release/ccsm")
}

/// A temp workspace with a .claude/sessions.json skeleton.
pub struct TempWorkspace {
    dir: tempfile::TempDir,
    home: PathBuf,
}

impl TempWorkspace {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("create .claude");

        // Write empty registry
        let registry = serde_json::json!({
            "updated": "",
            "sessions": []
        });
        std::fs::write(
            claude_dir.join("sessions.json"),
            serde_json::to_string_pretty(&registry).unwrap(),
        )
        .expect("write sessions.json");

        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".claude").join("sessions"))
            .expect("create sessions dir");

        Self { dir, home }
    }

    /// Workspace root path.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Run ccsm with args in this workspace. Returns (stdout, stderr, success).
    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(ccsm_binary())
            .args(args)
            .current_dir(self.path())
            .env("HOME", &self.home)
            .output()
            .expect("ccsm execution failed")
    }

    /// Run ccsm and expect success. Returns stdout as String.
    pub fn run_ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            panic!(
                "ccsm {:?} failed: {}",
                args,
                stderr
            );
        }
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// Run ccsm and expect failure. Returns stderr as String.
    pub fn run_err(&self, args: &[&str]) -> String {
        let out = self.run(args);
        if out.status.success() {
            panic!("ccsm {:?} unexpectedly succeeded", args);
        }
        String::from_utf8_lossy(&out.stderr).to_string()
    }

    /// Write session detail file content.
    pub fn write_detail(&self, name: &str, content: &str) {
        let sessions_dir = self.path().join(".claude").join("sessions");
        std::fs::create_dir_all(&sessions_dir).ok();
        std::fs::write(sessions_dir.join(format!("{name}.md")), content)
            .expect("write detail file");
    }

    /// Read the registry file.
    pub fn read_registry(&self) -> serde_json::Value {
        let path = self.path().join(".claude").join("sessions.json");
        let contents = std::fs::read_to_string(&path).expect("read registry");
        serde_json::from_str(&contents).expect("parse registry")
    }
}
