use crate::error::CmuxError;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::Mutex;
use tracing::{debug, info};

/// Configuration for spawning a PTY instance.
#[derive(Debug, Clone)]
pub struct ConPtyConfig {
    pub cols: u16,
    pub rows: u16,
    pub shell: String,
}

impl Default for ConPtyConfig {
    fn default() -> Self {
        Self {
            cols: cmux_config::defaults::DEFAULT_COLS,
            rows: cmux_config::defaults::DEFAULT_ROWS,
            shell: cmux_config::defaults::DEFAULT_SHELL.into(),
        }
    }
}

/// A PTY instance managing a single shell process.
///
/// Uses `portable-pty` for cross-platform ConPTY/Unix PTY support.
pub struct ConPty {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    reader: Mutex<Box<dyn Read + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
}

impl ConPty {
    /// Spawn a new PTY with the given configuration.
    pub fn spawn(config: &ConPtyConfig) -> Result<Self, CmuxError> {
        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(PtySize {
                rows: config.rows,
                cols: config.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| CmuxError::Pty(format!("openpty: {e}")))?;

        let parts: Vec<&str> = config.shell.split_whitespace().collect();
        let (program, args) = parts
            .split_first()
            .ok_or_else(|| CmuxError::Pty("empty shell command".into()))?;
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        cmd.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| CmuxError::Pty(format!("spawn_command: {e}")))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| CmuxError::Pty(format!("try_clone_reader: {e}")))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| CmuxError::Pty(format!("take_writer: {e}")))?;

        info!(
            shell = %config.shell,
            cols = config.cols,
            rows = config.rows,
            "PTY spawned"
        );

        Ok(Self {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
            child: Mutex::new(child),
        })
    }

    /// Read output from the PTY asynchronously.
    ///
    /// Uses `spawn_blocking` since pipe reads are synchronous.
    /// Returns 0 bytes when the pipe is closed (child exited).
    pub async fn read(&self, buf: &mut [u8]) -> Result<usize, CmuxError> {
        let len = buf.len();
        let buf_ptr = buf.as_mut_ptr() as usize;

        // We need to extract the reader from behind the Mutex to send to spawn_blocking.
        // Instead, read with a raw pointer approach.
        let reader_ptr = &self.reader as *const Mutex<Box<dyn Read + Send>> as usize;

        let result = tokio::task::spawn_blocking(move || -> Result<usize, CmuxError> {
            let reader_mutex = unsafe { &*(reader_ptr as *const Mutex<Box<dyn Read + Send>>) };
            let mut reader = reader_mutex
                .lock()
                .map_err(|e| CmuxError::Pty(format!("reader lock poisoned: {e}")))?;
            let slice = unsafe { std::slice::from_raw_parts_mut(buf_ptr as *mut u8, len) };
            match reader.read(slice) {
                Ok(n) => Ok(n),
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(0),
                Err(e) => Err(CmuxError::Io(e)),
            }
        })
        .await
        .map_err(|e| CmuxError::Pty(format!("spawn_blocking join: {e}")))?;

        result
    }

    /// Write input to the PTY asynchronously.
    pub async fn write(&self, data: &[u8]) -> Result<usize, CmuxError> {
        let data = data.to_vec();
        let writer_ptr = &self.writer as *const Mutex<Box<dyn Write + Send>> as usize;

        let result = tokio::task::spawn_blocking(move || -> Result<usize, CmuxError> {
            let writer_mutex = unsafe { &*(writer_ptr as *const Mutex<Box<dyn Write + Send>>) };
            let mut writer = writer_mutex
                .lock()
                .map_err(|e| CmuxError::Pty(format!("writer lock poisoned: {e}")))?;
            let n = writer.write(&data).map_err(CmuxError::Io)?;
            writer.flush().map_err(CmuxError::Io)?;
            Ok(n)
        })
        .await
        .map_err(|e| CmuxError::Pty(format!("spawn_blocking join: {e}")))?;

        result
    }

    /// Resize the PTY.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), CmuxError> {
        let master = self
            .master
            .lock()
            .map_err(|e| CmuxError::Pty(format!("master lock poisoned: {e}")))?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| CmuxError::Pty(format!("resize: {e}")))?;
        debug!(cols, rows, "PTY resized");
        Ok(())
    }

    /// Check if the child process has exited.
    pub fn try_wait(&self) -> Option<u32> {
        let mut child = self.child.lock().ok()?;
        match child.try_wait() {
            Ok(Some(status)) => Some(status.exit_code()),
            _ => None,
        }
    }

    /// Kill the child process.
    pub fn kill(&self) -> Result<(), CmuxError> {
        let mut child = self
            .child
            .lock()
            .map_err(|e| CmuxError::Pty(format!("child lock poisoned: {e}")))?;
        child
            .kill()
            .map_err(|e| CmuxError::Pty(format!("kill: {e}")))?;
        debug!("PTY child killed");
        Ok(())
    }
}
