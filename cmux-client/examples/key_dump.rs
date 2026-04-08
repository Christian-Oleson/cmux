//! Minimal standalone key event dumper for diagnosing cmux input issues.
//!
//! Usage:
//!   cargo run -p cmux-client --example key_dump 2> key_dump.log
//!
//! Then press keys (including Ctrl+B, %, arrow keys). Press Esc to exit.
//! Inspect key_dump.log to see exactly what events crossterm delivers.
//!
//! Uses blocking `event::read()` (not async EventStream) to isolate whether
//! the async path is the source of any input bugs.

use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::Write;

fn main() -> anyhow::Result<()> {
    enable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
    )?;

    let mut stderr = std::io::stderr().lock();
    writeln!(stderr, "key_dump: press keys, Esc to exit")?;
    stderr.flush()?;

    let result = (|| -> anyhow::Result<()> {
        loop {
            match event::read()? {
                Event::Key(k) => {
                    writeln!(stderr, "KEY {:?}", k)?;
                    stderr.flush()?;
                    if k.code == KeyCode::Esc {
                        break;
                    }
                }
                Event::Mouse(m) => {
                    writeln!(stderr, "MOUSE {:?}", m)?;
                    stderr.flush()?;
                }
                Event::Resize(c, r) => {
                    writeln!(stderr, "RESIZE {}x{}", c, r)?;
                    stderr.flush()?;
                }
                other => {
                    writeln!(stderr, "OTHER {:?}", other)?;
                    stderr.flush()?;
                }
            }
        }
        Ok(())
    })();

    // Always restore terminal state
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen,
    );
    let _ = disable_raw_mode();

    result
}
