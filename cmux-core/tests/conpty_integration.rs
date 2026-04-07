#[cfg(windows)]
mod tests {
    use cmux_core::pty::{ConPty, ConPtyConfig};
    use std::sync::Arc;
    use tokio::time::{timeout, Duration};

    /// Read from PTY until needle found or timeout.
    /// The reader thread is detached — it will terminate when the ConPty is dropped.
    async fn read_until_found(pty: &Arc<ConPty>, needle: &str, secs: u64) -> bool {
        let pty_clone = Arc::clone(pty);
        let needle = needle.to_string();

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

        // Detached reader thread — will block on read() until ConPty is dropped
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let mut buf = [0u8; 4096];
            loop {
                match rt.block_on(pty_clone.read(&mut buf)) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut all = Vec::new();
        timeout(Duration::from_secs(secs), async {
            while let Some(chunk) = rx.recv().await {
                all.extend_from_slice(&chunk);
                let text = String::from_utf8_lossy(&all);
                if text.contains(&needle) {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spawn_and_read_output() {
        let config = ConPtyConfig {
            cols: 80,
            rows: 24,
            shell: "cmd.exe /c echo hello_cmux_test".into(),
        };

        let pty = Arc::new(ConPty::spawn(&config).expect("Failed to spawn PTY"));
        assert!(
            read_until_found(&pty, "hello_cmux_test", 10).await,
            "Expected 'hello_cmux_test' in output"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn write_input_and_read() {
        let config = ConPtyConfig {
            cols: 80,
            rows: 24,
            shell: "cmd.exe".into(),
        };

        let pty = Arc::new(ConPty::spawn(&config).expect("Failed to spawn PTY"));

        // Wait for cmd.exe to start
        tokio::time::sleep(Duration::from_millis(500)).await;

        pty.write(b"echo test_input_works\r\n")
            .await
            .expect("write failed");

        assert!(
            read_until_found(&pty, "test_input_works", 10).await,
            "Expected 'test_input_works' in output"
        );

        let _ = pty.write(b"exit\r\n").await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resize_pty() {
        let config = ConPtyConfig {
            cols: 80,
            rows: 24,
            shell: "cmd.exe".into(),
        };

        let pty = ConPty::spawn(&config).expect("Failed to spawn PTY");
        pty.resize(120, 40).expect("resize failed");
        pty.resize(200, 50).expect("second resize failed");
        let _ = pty.write(b"exit\r\n").await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn child_exit_detection() {
        let config = ConPtyConfig {
            cols: 80,
            rows: 24,
            shell: "cmd.exe /c echo done".into(),
        };

        let pty = ConPty::spawn(&config).expect("Failed to spawn PTY");

        // Wait for child to finish
        tokio::time::sleep(Duration::from_secs(2)).await;

        let exit = pty.try_wait();
        assert!(exit.is_some(), "Expected child to have exited");
        assert_eq!(exit.unwrap(), 0, "Expected exit code 0");
    }
}
