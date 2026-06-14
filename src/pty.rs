use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::Write;
use std::path::Path;

/// Manages the PTY that embeds the Claude Code process.
pub struct Pty {
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn std::io::Write + Send>,
    reader_fd: std::os::unix::io::RawFd,
}

impl Pty {
    /// Spawn `claude` inside a new PTY in the given working directory.
    /// If `resume_session` is Some, passes `--resume <id>` to resume
    /// that conversation (claude loads the transcript from disk).
    pub fn spawn(
        rows: u16,
        cols: u16,
        cwd: &Path,
        resume_session: Option<&str>,
    ) -> Result<Self> {
        let pty_system = NativePtySystem::default();

        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open PTY pair")?;

        // Spawn claude directly — env vars (ANTHROPIC_BASE_URL, ANTHROPIC_AUTH_TOKEN,
        // etc.) are inherited from cc-tui's parent process. The user must export
        // them in their shell config (e.g., fish: set -gx ANTHROPIC_BASE_URL ...).
        let mut cmd = CommandBuilder::new("claude");
        if let Some(sid) = resume_session {
            cmd.arg("--resume");
            cmd.arg(sid);
        }
        cmd.cwd(cwd);
        let child = pty_pair
            .slave
            .spawn_command(cmd)
            .context("failed to spawn claude process")?;

        // Close slave FD — only the child process needs it
        drop(pty_pair.slave);

        let master = pty_pair.master;

        // Get the raw file descriptor for non-blocking reads
        let reader_fd = master
            .as_raw_fd()
            .context("PTY master has no raw file descriptor")?;

        // Set non-blocking on the master FD
        unsafe {
            let flags = libc::fcntl(reader_fd, libc::F_GETFL, 0);
            libc::fcntl(reader_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }

        // Take the writer — can only be called once per MasterPty
        let writer = master
            .take_writer()
            .context("failed to take PTY writer")?;

        Ok(Self {
            master,
            child,
            writer,
            reader_fd,
        })
    }

    /// Read available output from the PTY. Returns number of bytes read, or 0 if no data.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let n = unsafe {
            libc::read(
                self.reader_fd,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
            )
        };
        if n >= 0 {
            Ok(n as usize)
        } else {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EAGAIN)
                || err.raw_os_error() == Some(libc::EWOULDBLOCK)
            {
                Ok(0)
            } else {
                Err(err).context("PTY read error")
            }
        }
    }

    /// Write input to the PTY (keyboard passthrough to claude).
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.writer
            .write_all(data)
            .context("failed to write to PTY")
    }

    /// Resize the PTY window (called on terminal resize).
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to resize PTY")
    }

    /// Check if the child process has exited.
    pub fn try_wait(&mut self) -> Option<portable_pty::ExitStatus> {
        self.child.try_wait().unwrap_or(None)
    }

    /// Kill the child process (SIGKILL — non-trappable, prevents state saving).
    pub fn kill(&mut self) -> Result<()> {
        self.child.kill().context("failed to kill child process")
    }

    /// Return the child process ID, if available.
    pub fn pid(&self) -> Option<u32> {
        self.child.process_id()
    }

    // detach() removed — cc-tui now kills the child on exit.
    // Transcripts are saved incrementally by Claude, so nothing is lost.
}
