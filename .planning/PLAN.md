<?xml version="1.0" encoding="UTF-8"?>
<!-- Dos Apes Super Agent Framework - Phase Plan -->
<!-- Generated: 2026-04-08 -->
<!-- Phase: 9 -->

<plan>
  <metadata>
    <phase>9</phase>
    <name>CLI Polish, Error Handling &amp; Distribution</name>
    <goal>Fix the interactive client's Windows input/rendering bugs, complete the CLI, add structured logging, ship a release build with a proper README</goal>
    <deliverable>A shippable v1.0 binary where both the interactive client AND the JSON-RPC API are usable, with a README, CI, and release artifacts</deliverable>
    <created>2026-04-08</created>
  </metadata>

  <context>
    <dependencies>Phase 8 complete — all runtime features present, 168 tests, JSON-RPC API working end-to-end</dependencies>
    <affected_areas>
      - cmux-client: MAJOR — fix Windows key input handling, add alternate screen buffer, handle reattach screen state
      - cmux-daemon: send pane screen snapshots on attach (currently only sends workspace structure)
      - cmux-daemon/src/main.rs: graceful shutdown, structured file logging
      - cmux-client/src/main.rs: more CLI subcommands (detach, list-panes, list-windows, rename, display-message)
      - .github/workflows/: CI + release workflows
      - README.md: user-facing documentation
      - cmux-client/src/terminal.rs: REMOVE the debug_key_log scaffolding added during Phase 8 debugging
    </affected_areas>
    <patterns_to_follow>
      - The interactive client is the user-facing pain point from Phase 8 — take it seriously with a diagnostic-first approach
      - JSON-RPC path is already tested and working — do not break it
      - "Done" for Phase 9 means a user can run `cmux-daemon` + `cmux-client new -s main` in Windows Terminal/PowerShell and get a working multiplexer, OR use the JSON-RPC API from a script
      - Deferred features (.msi installer, session persistence across daemon restarts, OSC notifications, prefix+: command mode) are NOT in scope for Phase 9 — they're post-v1.0 polish
    </patterns_to_follow>
  </context>

  <tasks>
    <task id="1" type="backend" complete="false">
      <name>Diagnostic-first fix of the interactive client's Windows input + rendering</name>
      <description>
        The blocker from Phase 8 user testing: pressing Ctrl+B in the interactive
        client did not trigger prefix mode, and on reattach the screen showed
        stale/garbled content because the daemon doesn't replay screen state.
        This task fixes all of it with a diagnostic-first approach: build a
        minimal crossterm event dumper to identify what Windows is actually
        sending, then fix the client based on real data.
      </description>

      <files>
        <create>
          cmux-client/examples/key_dump.rs      (minimal standalone crossterm event logger)
        </create>
        <modify>
          cmux-client/src/terminal.rs           (remove debug_key_log, add alternate screen buffer, improve input handling based on diagnostic findings)
          cmux-client/src/pane_manager.rs       (remove #[allow(dead_code)] fallout, clean up any Phase 6-8 dead code)
          cmux-daemon/src/session_manager.rs    (add read_pane_snapshot returning ScreenSnapshot bytes for replay)
          cmux-daemon/src/server.rs             (on Attach, send pane snapshots as synthesized PaneOutput messages so client's local screen buffers get populated)
          cmux-ipc/src/messages.rs              (optionally add PaneSnapshot message variant — OR just reuse PaneOutput with the raw VT stream)
        </create>
      </files>

      <action>
        **PART A — Diagnostic: build a standalone key dumper**

        1. Create cmux-client/examples/key_dump.rs — a minimal standalone program
           that enables raw mode + alternate screen + mouse capture, reads events
           from crossterm's blocking `event::read()` loop, and prints each event
           to stderr (which we pipe to a file). Exits on Esc.

           ```rust
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

               disable_raw_mode()?;
               crossterm::execute!(
                   std::io::stdout(),
                   crossterm::event::DisableMouseCapture,
                   crossterm::terminal::LeaveAlternateScreen,
               )?;
               Ok(())
           }
           ```

           User runs: `cargo run -p cmux-client --example key_dump 2&gt; key_dump.log`
           Then presses Ctrl+B, %, arrow keys, Esc.
           The log shows exactly what crossterm sees.

        2. Based on the key_dump output, identify the actual bug. Likely candidates:
           - crossterm feature flag missing (`bracketed-paste`, `event-stream`, `events`)
           - Windows Terminal with `KeyboardEnhancementFlags` required
           - EventStream (async) behaves differently from blocking event::read()
           - Events are going via `KeyEventKind::Press` but the `kind` field is
             `NoKind` on some Windows terminals (cargo/crossterm bug)
           - The `key_event_to_bytes` fallthrough `_ => None` eating something

        **PART B — Fix the identified bug**

        3. Apply the fix. Possibilities:
           - If EventStream doesn't deliver all events: switch to a blocking-thread-
             with-mpsc pattern instead of crossterm's async EventStream
           - If `kind` is `NoKind`: treat it the same as `Press`
           - If enhancement flags are needed: call `PushKeyboardEnhancementFlags`
             on enter, `PopKeyboardEnhancementFlags` on exit
           - If case normalization wasn't enough: also normalize Shift+Char mappings

        4. Remove the `debug_key_log` scaffolding and file path from terminal.rs.
           (No debug file in production binary.)

        **PART C — Alternate screen buffer**

        5. In `RawModeGuard::enable()`, also enter the alternate screen buffer:
           ```rust
           crossterm::execute!(
               std::io::stdout(),
               crossterm::terminal::EnterAlternateScreen,
           )?;
           ```
           
        6. In `RawModeGuard::drop()`, leave the alternate screen buffer (it's
           already there per current code — verify it runs). This hides all the
           host terminal's pre-cmux content while cmux is running and restores
           it cleanly on exit.

        **PART D — Reattach screen state**

        7. In `cmux-daemon/src/session_manager.rs`, add:
           ```rust
           /// Return the raw VT content that would reconstruct the current pane
           /// screen state. Uses vt100's `contents_formatted()`.
           pub async fn pane_snapshot_bytes(
               &amp;self,
               session_name: &amp;str,
               pane_id: u32,
           ) -&gt; Result&lt;Vec&lt;u8&gt;, CmuxError&gt; {
               let sessions = self.sessions.lock().await;
               let session = sessions.get(session_name)
                   .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;
               for ws in session.workspaces.values() {
                   if let Some(pane) = ws.panes.get(&amp;pane_id) {
                       let screen = pane.screen.lock().await;
                       return Ok(screen.contents_formatted());
                   }
               }
               Err(CmuxError::PaneNotFound(cmux_core::types::PaneId(pane_id)))
           }
           ```
           Also expose `contents_formatted()` on cmux-core::screen::ScreenBuffer if
           it doesn't exist — vt100::Screen has a method by that name that returns
           the screen state as a byte string with escape sequences.

        8. In `cmux-daemon/src/server.rs`, after sending `SessionState` on Attach
           and on CreateSession, iterate over all panes in the session and send
           one `ServerMessage::PaneOutput { pane_id, data: snapshot_bytes }` per
           pane. This makes the client's local screen buffer populate immediately.

        9. Sanity check: the client's existing `process_output` path will feed
           those bytes into its per-pane ScreenBuffer, and render_full will then
           draw the restored state. No new client code needed.
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo test --workspace</command>
        <manual>
          1. Run: cargo run -p cmux-client --example key_dump 2&gt; key_dump.log
             Press Ctrl+B, %, arrow keys, Esc. Inspect key_dump.log to verify
             crossterm is actually delivering events.
          2. Start daemon, start interactive client with `new -s main`.
          3. Verify alternate screen buffer is entered (host terminal content
             disappears, cmux takes over the whole window).
          4. Press Ctrl+B, release, Shift+5 → pane should split vertically.
          5. Type in each pane → only active pane receives input.
          6. Ctrl+B d → detaches, host terminal restored.
          7. cargo run -p cmux-client -- attach -t main → reattach shows
             the actual pane content restored (not blank).
        </manual>
      </verification>

      <done>
        - key_dump.log shows clear evidence of what's happening on Windows
        - Ctrl+B prefix detection works end-to-end
        - Alternate screen buffer is used (host terminal preserved)
        - Reattach restores visible pane content from daemon's ScreenBuffer
        - debug_key_log scaffolding removed
        - All existing tests still pass
      </done>
    </task>

    <task id="2" type="backend" complete="false">
      <name>CLI command completeness, daemon graceful shutdown, and file logging</name>
      <description>
        Round out the CLI with the commands listed in REQUIREMENTS.md Section 9.1
        that aren't yet present. Add graceful shutdown handling to the daemon so
        Ctrl+C cleanly kills all PTYs. Route daemon logs to %APPDATA%\cmux\cmux.log
        via tracing-appender.
      </description>

      <files>
        <modify>
          cmux-client/src/main.rs               (add detach, list-panes, list-windows, rename-session, rename-window, send-keys, display-message, kill-server subcommands)
          cmux-daemon/src/main.rs                (graceful shutdown via tokio::signal::ctrl_c(), file logging via tracing-appender)
          cmux-daemon/src/session_manager.rs     (add shutdown_all() that kills every pane across all sessions cleanly)
          cmux-daemon/Cargo.toml                (verify tracing-appender is already a dep — it was added in Phase 1)
        </modify>
      </files>

      <action>
        1. **CLI additions** in cmux-client/src/main.rs (via clap subcommands):
           - `detach` — send Detach message from CLI context (rarely needed, usually done via keybinding, but per spec)
           - `list-panes -t &lt;session&gt;` — call GetSessionState, pretty-print all pane IDs + workspace mapping
           - `list-windows -t &lt;session&gt;` — same but workspaces only
           - `rename-session -t &lt;old&gt; -n &lt;new&gt;` — for now, log "not implemented" and exit 1 (real rename needs daemon support, deferred)
           - `send-keys -t &lt;session&gt; -p &lt;pane&gt; &lt;keys...&gt;` — create a one-shot connection, send PaneInput, disconnect
           - `kill-server` — tells daemon to shut down (new ClientMessage::KillServer + daemon-side handler that triggers tokio::process::exit)
           - `display-message &lt;msg&gt;` — deferred, log "not implemented"

           The subcommands that work end-to-end: `list-panes`, `list-windows`, `send-keys`, `kill-server`.
           The rest print a polite "not yet implemented" so the CLI is complete even if some commands are stubs.

        2. **Graceful shutdown** in cmux-daemon/src/main.rs:
           ```rust
           let shutdown_sm = Arc::clone(&amp;session_manager);
           tokio::select! {
               r = interactive =&gt; { r??; }
               r = rpc =&gt; { r??; }
               _ = tokio::signal::ctrl_c() =&gt; {
                   info!("Received Ctrl+C, shutting down");
                   shutdown_sm.shutdown_all().await;
               }
           }
           ```
           Add `shutdown_all()` to SessionManager that iterates all sessions and
           kills every ConPty before returning.

        3. **File logging** in cmux-daemon/src/main.rs:
           Use tracing-appender's rolling file to %APPDATA%\cmux\cmux.log:
           ```rust
           let log_dir = dirs::data_dir()
               .map(|d| d.join("cmux"))
               .unwrap_or_else(|| std::env::temp_dir().join("cmux"));
           std::fs::create_dir_all(&amp;log_dir).ok();
           let file_appender = tracing_appender::rolling::daily(&amp;log_dir, "cmux.log");
           let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

           tracing_subscriber::fmt()
               .with_writer(non_blocking)
               .with_env_filter(...)
               .init();
           ```
           Keep the `_guard` alive in main's scope (drop flushes).

           Note: add `dirs = "5"` to cmux-daemon's dependencies if not already there.

        4. **Error handling improvements** in cmux-daemon/src/server.rs and
           cmux-daemon/src/session_manager.rs:
           - When a ConPty's reader task detects EOF (child exited), broadcast a
             PaneOutput with a final "[process exited with code N]\r\n" line so
             attached clients see it.
             Requires ConPty::try_wait() to get exit code. Already exists.
           - When a client pipe read fails, log the error and continue serving
             (current behavior is to break the loop — that's correct).
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo test --workspace</command>
        <manual>
          1. Start daemon, check %APPDATA%\cmux\cmux.log.YYYY-MM-DD is being written.
          2. cargo run -p cmux-client -- new -s main → press Ctrl+B % to split
          3. cargo run -p cmux-client -- list-panes -t main → shows 2 panes
          4. cargo run -p cmux-client -- send-keys -t main -p 0 "echo test"
          5. In daemon terminal: press Ctrl+C → daemon logs "shutting down",
             kills all PTYs, exits cleanly (no orphaned powershell.exe processes
             in Task Manager)
        </manual>
      </verification>

      <done>
        - CLI has all commands from REQUIREMENTS.md Section 9.1 (real or stubbed)
        - Daemon logs to %APPDATA%\cmux\cmux.log with daily rotation
        - Ctrl+C on daemon triggers graceful shutdown — no orphaned PTYs
        - Child process exit codes displayed in panes
        - All tests still pass
      </done>
    </task>

    <task id="3" type="deploy" complete="false">
      <name>README, GitHub Actions CI, and release artifacts</name>
      <description>
        Write the user-facing README with installation, quickstart, keybindings,
        and JSON-RPC API reference. Set up GitHub Actions CI on windows-latest
        running build/test/clippy/fmt. Add a release workflow that produces
        standalone .exe artifacts.
      </description>

      <files>
        <create>
          README.md                              (top-level user-facing docs)
          .github/workflows/ci.yml              (build, test, clippy, fmt on windows-latest)
          .github/workflows/release.yml         (release build + upload .exe artifacts on tag push)
        </create>
        <modify>
          Cargo.toml                             (add workspace.package metadata: authors, license, repository, description)
        </modify>
      </files>

      <action>
        1. **README.md** with the following sections:
           - What is cmux (one paragraph)
           - Status badge row (CI status, crate version — placeholder for now)
           - Installation (build from source: `cargo build --release`)
           - Quickstart: start daemon + client in two terminals, basic prefix
             keys (the tmux-style two-step)
           - Keybinding reference table
           - Configuration example (link to cmux-config/example/cmux.toml)
           - JSON-RPC API overview with a Python + PowerShell example
             (reference scripts/cmux-rpc.ps1)
           - Architecture diagram (ASCII art showing daemon/client/RPC pipe)
           - Known limitations (list tech debt: scrollback search stubbed, 
             no MSI installer yet, copy-mode scrollback nav, daemon session
             persistence across restarts)
           - Building + testing (cargo build/test/clippy commands)
           - Contributing (link to REQUIREMENTS.md and .planning/ROADMAP.md)
           - License (MIT or Apache-2.0 — match what's in Cargo.toml)

        2. **.github/workflows/ci.yml**:
           ```yaml
           name: CI
           on:
             push:
               branches: [main]
             pull_request:
               branches: [main]

           jobs:
             build:
               runs-on: windows-latest
               steps:
                 - uses: actions/checkout@v4
                 - uses: dtolnay/rust-toolchain@stable
                   with:
                     components: rustfmt, clippy
                 - uses: Swatinem/rust-cache@v2
                 - name: Check formatting
                   run: cargo fmt --all --check
                 - name: Clippy
                   run: cargo clippy --workspace -- -D warnings
                 - name: Build
                   run: cargo build --workspace
                 - name: Test
                   run: cargo test --workspace
           ```

        3. **.github/workflows/release.yml**:
           ```yaml
           name: Release
           on:
             push:
               tags: ['v*']

           jobs:
             release:
               runs-on: windows-latest
               steps:
                 - uses: actions/checkout@v4
                 - uses: dtolnay/rust-toolchain@stable
                 - uses: Swatinem/rust-cache@v2
                 - name: Build release
                   run: cargo build --release --workspace
                 - name: Package
                   shell: pwsh
                   run: |
                     New-Item -ItemType Directory -Path release | Out-Null
                     Copy-Item target/release/cmux-daemon.exe release/
                     Copy-Item target/release/cmux-client.exe release/
                     Copy-Item README.md release/
                     Copy-Item cmux-config/example/cmux.toml release/cmux.example.toml
                     Compress-Archive -Path release/* -DestinationPath cmux-$env:GITHUB_REF_NAME-windows-x64.zip
                 - name: Upload release
                   uses: softprops/action-gh-release@v2
                   with:
                     files: cmux-*.zip
           ```

        4. **Workspace metadata** in root Cargo.toml:
           ```toml
           [workspace.package]
           version = "0.1.0"
           edition = "2021"
           license = "MIT OR Apache-2.0"
           repository = "https://github.com/USER/cmux"
           description = "Native Windows terminal multiplexer"
           ```
           And each crate's Cargo.toml picks these up with `*.workspace = true`.
           (Optional — only if the user wants to publish to crates.io eventually.)

        5. Verify `cargo build --release` produces two exes in `target/release/`
           that can run standalone (no cargo needed).
      </action>

      <verification>
        <command>cargo build --release --workspace</command>
        <command>cargo test --workspace</command>
        <manual>
          1. README renders correctly on GitHub preview
          2. target/release/cmux-daemon.exe runs standalone
          3. target/release/cmux-client.exe runs standalone
          4. Push a test tag: git tag v0.0.1-test &amp;&amp; git push --tags
             — verify release workflow triggers in GitHub Actions (if repo pushed)
        </manual>
      </verification>

      <done>
        - README.md exists and accurately describes the project
        - CI workflow passes on windows-latest
        - Release workflow builds standalone .exe artifacts
        - Workspace Cargo.toml has shared metadata
        - `cargo build --release` produces working standalone binaries
      </done>
    </task>
  </tasks>

  <phase_verification>
    <commands>
      <command>cargo build --workspace</command>
      <command>cargo build --release --workspace</command>
      <command>cargo clippy --workspace -- -D warnings</command>
      <command>cargo fmt --all --check</command>
      <command>cargo test --workspace</command>
    </commands>
    <manual>
      1. Fresh install: cargo clean, cargo build --release
      2. Start daemon: .\target\release\cmux-daemon.exe
      3. Start client: .\target\release\cmux-client.exe new -s main
      4. Interactive client actually works: Ctrl+B %, arrow navigate, Ctrl+B d
      5. JSON-RPC path still works via scripts/cmux-rpc.ps1
      6. Daemon Ctrl+C is clean (no orphaned powershell.exe)
      7. Check %APPDATA%\cmux\cmux.log.YYYY-MM-DD has logs
      8. README is accurate and matches what actually works
    </manual>
  </phase_verification>

  <completion_criteria>
    <criterion>All 3 tasks marked complete</criterion>
    <criterion>cargo build/clippy/fmt/test all pass</criterion>
    <criterion>Interactive client works end-to-end (Ctrl+B prefix, split, navigate, detach/reattach)</criterion>
    <criterion>JSON-RPC path still works (Phase 8 regression check)</criterion>
    <criterion>Release build produces standalone .exe</criterion>
    <criterion>README covers installation, quickstart, keybindings, and JSON-RPC API</criterion>
    <criterion>CI workflow runs on windows-latest</criterion>
    <criterion>Daemon graceful shutdown on Ctrl+C</criterion>
    <criterion>No debug_key_log scaffolding left in production code</criterion>
  </completion_criteria>

  <deferred>
    Explicitly NOT in Phase 9 scope (post-v1.0):
    - MSI installer / WinGet / Scoop / Chocolatey packages
    - Session state persistence across daemon restarts
    - OSC 9/99/777 toast notifications
    - Interactive command mode (prefix + :)
    - rename-session / rename-window real implementations
    - Scrollback search in copy mode
    - Scrollback history navigation in copy mode (vt100::Screen scroll_up)
    - JSON-RPC event streaming subscriptions
    - Cross-platform Linux/macOS support
  </deferred>
</plan>
