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

/// A temp workspace with ccsm identity + global data directory.
pub struct TempWorkspace {
    dir: tempfile::TempDir,
    home: PathBuf,
}

impl TempWorkspace {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path().join("home");

        // Create identity file at workspace root (a FILE named .ccsm)
        let id = format!("{:x}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos());
        let identity_path = dir.path().join(".ccsm");
        std::fs::write(
            &identity_path,
            format!("version = \"1\"\nid = \"{id}\"\n"),
        )
        .expect("write .ccsm identity");

        // Create global data directory at $HOME/.ccsm/<id>/
        let global_dir = home.join(".ccsm").join(&id);
        std::fs::create_dir_all(&global_dir).expect("create global dir");

        // Write empty registry
        let registry = serde_json::json!({
            "updated": "",
            "sessions": []
        });
        std::fs::write(
            global_dir.join("sessions.json"),
            serde_json::to_string_pretty(&registry).unwrap(),
        )
        .expect("write sessions.json");

        std::fs::create_dir_all(home.join(".claude").join("sessions"))
            .expect("create sessions dir");

        Self { dir, home }
    }

    /// Workspace root path.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Home directory path (used as $HOME in tests).
    pub fn home(&self) -> PathBuf {
        self.home.clone()
    }

    /// Run ccsm with args in this workspace. Returns (stdout, stderr, success).
    /// Clears CCSM_SESSION and CCSM_WORKTREE to prevent parent env var leaks.
    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(ccsm_binary())
            .args(args)
            .current_dir(self.path())
            .env_remove("CCSM_SESSION")
            .env_remove("CCSM_WORKTREE")
            .env_remove("CCSM_WORKSPACE")
            .env("HOME", &self.home)
            .output()
            .expect("ccsm execution failed")
    }

    /// Read identity file content as string.
    pub fn read_identity(&self) -> String {
        let path = self.path().join(".ccsm");
        std::fs::read_to_string(&path).expect("read .ccsm identity")
    }

    /// Overwrite the identity version field. Preserves id.
    pub fn set_identity_version(&self, version: &str) {
        let content = self.read_identity();
        let id = content
            .lines()
            .find_map(|l| l.strip_prefix("id = \"").and_then(|s| s.strip_suffix('"')))
            .expect("parse identity id");
        let identity_path = self.path().join(".ccsm");
        std::fs::write(&identity_path, format!("version = \"{version}\"\nid = \"{id}\"\n"))
            .expect("write updated identity");
    }

    /// Run ccsm and return stderr as String (regardless of exit status).
    pub fn run_stderr(&self, args: &[&str]) -> String {
        let out = self.run(args);
        String::from_utf8_lossy(&out.stderr).to_string()
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

    /// Resolve the global data dir from the identity file.
    fn global_dir(&self) -> PathBuf {
        let identity = self.path().join(".ccsm");
        let content = std::fs::read_to_string(&identity).expect("read .ccsm identity");
        let id = content
            .lines()
            .find_map(|l| l.strip_prefix("id = \"").and_then(|s| s.strip_suffix('"')))
            .expect("parse identity id")
            .to_string();
        self.home.join(".ccsm").join(id)
    }

    /// Write session detail file content.
    #[allow(dead_code)]
    pub fn write_detail(&self, name: &str, content: &str) {
        let sessions_dir = self.global_dir().join("sessions");
        std::fs::create_dir_all(&sessions_dir).ok();
        std::fs::write(sessions_dir.join(format!("{name}.md")), content)
            .expect("write detail file");
    }

    /// Read the registry file from global data dir.
    pub fn read_registry(&self) -> serde_json::Value {
        let path = self.global_dir().join("sessions.json");
        let contents = std::fs::read_to_string(&path).expect("read registry");
        serde_json::from_str(&contents).expect("parse registry")
    }
}
