/// Default prefix key (Ctrl+B, tmux-compatible).
pub const DEFAULT_PREFIX_KEY: &str = "C-b";

/// Default shell to spawn in panes.
#[cfg(windows)]
pub const DEFAULT_SHELL: &str = "powershell.exe";

#[cfg(not(windows))]
pub const DEFAULT_SHELL: &str = "/bin/sh";

/// Default scrollback buffer size in lines.
pub const DEFAULT_SCROLLBACK: usize = 10_000;

/// Default pane dimensions.
pub const DEFAULT_COLS: u16 = 80;
pub const DEFAULT_ROWS: u16 = 24;

/// Named pipe path for IPC.
pub const PIPE_NAME: &str = r"\\.\pipe\cmux";

/// Named pipe path for JSON-RPC API.
pub const RPC_PIPE_NAME: &str = r"\\.\pipe\cmux-rpc";

/// Default escape time (prefix key timeout) in milliseconds.
pub const DEFAULT_ESCAPE_TIME_MS: u64 = 500;

/// Default base index for window/pane numbering.
pub const DEFAULT_BASE_INDEX: u32 = 0;
