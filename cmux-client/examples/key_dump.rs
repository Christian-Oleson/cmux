//! Minimal standalone key event dumper for diagnosing cmux input issues.
//!
//! Usage:
//!   cargo run -p cmux-client --example key_dump
//!
//! Writes every crossterm event to stdout live AND to key_dump.log in the
//! current directory. Press Esc to exit.
//!
//! Uses blocking `event::read()` (not async EventStream) to isolate whether
//! the async path is the source of any input bugs.

use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::fs::OpenOptions;
use std::io::Write;

fn main() -> anyhow::Result<()> {
    // Open log file up front
    let mut log = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("key_dump.log")?;

    writeln!(log, "key_dump started")?;
    log.flush()?;

    println!("========================================");
    println!(" cmux key_dump diagnostic");
    println!("========================================");
    println!();
    println!("Press keys to see what crossterm delivers.");
    println!("All events are written to key_dump.log");
    println!("Press Esc to exit.");
    println!();

    // Enable raw mode so we receive keys (no alt screen so host terminal
    // keeps showing what we print).
    enable_raw_mode()?;

    let result = (|| -> anyhow::Result<()> {
        loop {
            let ev = event::read()?;
            // Print to the host terminal so it is visible live, plus log it
            // to the file for copy-paste.
            let line = format!("{:?}", ev);
            // `\r\n` needed in raw mode so the cursor returns to col 0
            print!("{}\r\n", line);
            std::io::stdout().flush().ok();
            writeln!(log, "{}", line)?;
            log.flush()?;

            if let Event::Key(k) = &ev {
                if k.code == KeyCode::Esc {
                    break;
                }
            }
        }
        Ok(())
    })();

    disable_raw_mode()?;

    writeln!(log, "key_dump exited")?;
    log.flush()?;

    println!();
    println!("========================================");
    println!(" Log written to: {}", std::env::current_dir()?.join("key_dump.log").display());
    println!("========================================");

    result
}
