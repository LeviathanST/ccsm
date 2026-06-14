use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::Write;

/// Manages the PTY that embeds the Claude Code process.
pub struct Pty {
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn std::io::Write + Send>,
    reader_fd: std::os::unix::io::RawFd,
}

impl Pty {
    /// Spawn `claude` inside a new PTY. Returns the PTY handle.
    pub fn spawn(rows: u16, cols: u16) -> Result<Self> {
        let pty_system = NativePtySystem::default();

        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open PTY pair")?;

        // Spawn through fish to pick up the `cds` function
        // (sets Anthropic env vars before launching claude)
        let mut cmd = CommandBuilder::new("fish");
        cmd.arg("-c");
        cmd.arg("cds");
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

    /// Kill the child process.
    pub fn kill(&mut self) -> Result<()> {
        self.child.kill().context("failed to kill child process")
    }
}
