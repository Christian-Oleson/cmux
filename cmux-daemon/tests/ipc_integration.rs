#[cfg(windows)]
mod tests {
    use cmux_ipc::messages::{ClientMessage, ServerMessage};
    use cmux_ipc::transport;
    use tokio::net::windows::named_pipe::{ClientOptions, PipeMode, ServerOptions};
    use tokio::time::{timeout, Duration};

    /// Use a unique pipe name per test to avoid conflicts.
    fn test_pipe_name(suffix: &str) -> String {
        format!(r"\\.\pipe\cmux_test_{}", suffix)
    }

    #[tokio::test]
    async fn transport_round_trip_over_named_pipe() {
        let pipe_name = test_pipe_name("round_trip");

        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .pipe_mode(PipeMode::Byte)
            .create(&pipe_name)
            .unwrap();

        // Client connects in a separate task
        let pipe_name_clone = pipe_name.clone();
        let client_task = tokio::spawn(async move {
            // Small delay to let server start
            tokio::time::sleep(Duration::from_millis(100)).await;
            let mut client = ClientOptions::new().open(&pipe_name_clone).unwrap();

            // Send a message
            let msg = ClientMessage::CreateSession {
                name: "test".into(),
            };
            transport::write_message(&mut client, &msg).await.unwrap();

            // Read response
            let resp: Option<ServerMessage> = transport::read_message(&mut client).await.unwrap();
            resp
        });

        // Server accepts and responds
        server.connect().await.unwrap();
        let msg: Option<ClientMessage> = transport::read_message(&mut server).await.unwrap();

        assert!(msg.is_some());
        match msg.unwrap() {
            ClientMessage::CreateSession { name } => assert_eq!(name, "test"),
            _ => panic!("wrong message type"),
        }

        let resp = ServerMessage::SessionCreated {
            id: 0,
            name: "test".into(),
        };
        transport::write_message(&mut server, &resp).await.unwrap();

        // Client should have received the response
        let client_resp = timeout(Duration::from_secs(5), client_task)
            .await
            .expect("client timed out")
            .expect("client task panicked");

        match client_resp.unwrap() {
            ServerMessage::SessionCreated { id, name } => {
                assert_eq!(id, 0);
                assert_eq!(name, "test");
            }
            _ => panic!("wrong response type"),
        }
    }

    #[tokio::test]
    async fn multiple_messages_sequentially() {
        let pipe_name = test_pipe_name("multi_msg");

        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .pipe_mode(PipeMode::Byte)
            .create(&pipe_name)
            .unwrap();

        let pipe_name_clone = pipe_name.clone();
        let client_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let mut client = ClientOptions::new().open(&pipe_name_clone).unwrap();

            // Send list sessions
            transport::write_message(&mut client, &ClientMessage::ListSessions)
                .await
                .unwrap();

            // Read response
            let resp: Option<ServerMessage> = transport::read_message(&mut client).await.unwrap();
            assert!(resp.is_some());
            match resp.unwrap() {
                ServerMessage::SessionList { sessions } => {
                    assert!(sessions.is_empty());
                }
                _ => panic!("wrong response"),
            }

            // Send another message
            transport::write_message(
                &mut client,
                &ClientMessage::KillSession {
                    name: "nonexistent".into(),
                },
            )
            .await
            .unwrap();

            let resp: Option<ServerMessage> = transport::read_message(&mut client).await.unwrap();
            assert!(resp.is_some());
            match resp.unwrap() {
                ServerMessage::Error { message } => {
                    assert!(message.contains("nonexistent"));
                }
                _ => panic!("expected error"),
            }
        });

        server.connect().await.unwrap();

        // Handle message 1: ListSessions
        let msg: Option<ClientMessage> = transport::read_message(&mut server).await.unwrap();
        assert!(matches!(msg, Some(ClientMessage::ListSessions)));
        transport::write_message(
            &mut server,
            &ServerMessage::SessionList { sessions: vec![] },
        )
        .await
        .unwrap();

        // Handle message 2: KillSession
        let msg: Option<ClientMessage> = transport::read_message(&mut server).await.unwrap();
        assert!(matches!(msg, Some(ClientMessage::KillSession { .. })));
        transport::write_message(
            &mut server,
            &ServerMessage::Error {
                message: "Session not found: nonexistent".into(),
            },
        )
        .await
        .unwrap();

        timeout(Duration::from_secs(5), client_task)
            .await
            .expect("client timed out")
            .expect("client task panicked");
    }

    #[tokio::test]
    async fn pane_input_binary_data() {
        let pipe_name = test_pipe_name("binary");

        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .pipe_mode(PipeMode::Byte)
            .create(&pipe_name)
            .unwrap();

        let pipe_name_clone = pipe_name.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let mut client = ClientOptions::new().open(&pipe_name_clone).unwrap();

            // Send binary data (ESC sequences, null bytes)
            let msg = ClientMessage::PaneInput {
                pane_id: 0,
                data: vec![0x1b, 0x5b, 0x41, 0x00, 0xff],
            };
            transport::write_message(&mut client, &msg).await.unwrap();
        });

        server.connect().await.unwrap();
        let msg: Option<ClientMessage> =
            timeout(Duration::from_secs(5), transport::read_message(&mut server))
                .await
                .expect("timed out")
                .unwrap();

        match msg.unwrap() {
            ClientMessage::PaneInput { pane_id, data } => {
                assert_eq!(pane_id, 0);
                assert_eq!(data, vec![0x1b, 0x5b, 0x41, 0x00, 0xff]);
            }
            _ => panic!("wrong message type"),
        }
    }
}
