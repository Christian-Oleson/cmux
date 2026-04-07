use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Write a length-prefixed JSON message to an async writer.
///
/// Format: 4-byte little-endian u32 length, then JSON bytes.
pub async fn write_message<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    msg: &T,
) -> std::io::Result<()> {
    let json = serde_json::to_vec(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let len = json.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(&json).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a length-prefixed JSON message from an async reader.
///
/// Returns `None` if the connection is closed (zero-length read).
pub async fn read_message<R: AsyncRead + Unpin, T: DeserializeOwned>(
    reader: &mut R,
) -> std::io::Result<Option<T>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > 16 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("message too large: {len} bytes"),
        ));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;

    let msg = serde_json::from_slice(&buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Some(msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{ClientMessage, ServerMessage};

    #[tokio::test]
    async fn round_trip_client_message() {
        let (client, server) = tokio::io::duplex(1024);
        let (mut cw, mut sr) = (client, server);

        let msg = ClientMessage::CreateSession {
            name: "test".into(),
        };
        write_message(&mut cw, &msg).await.unwrap();
        drop(cw);

        let received: Option<ClientMessage> = read_message(&mut sr).await.unwrap();
        assert!(received.is_some());
        match received.unwrap() {
            ClientMessage::CreateSession { name } => assert_eq!(name, "test"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn round_trip_server_message() {
        let (mut w, mut r) = tokio::io::duplex(1024);

        let msg = ServerMessage::Ok;
        write_message(&mut w, &msg).await.unwrap();
        drop(w);

        let received: Option<ServerMessage> = read_message(&mut r).await.unwrap();
        assert!(received.is_some());
    }

    #[tokio::test]
    async fn multiple_messages() {
        let (mut w, mut r) = tokio::io::duplex(4096);

        for i in 0..5 {
            let msg = ClientMessage::PaneInput {
                pane_id: i,
                data: vec![b'a' + i as u8],
            };
            write_message(&mut w, &msg).await.unwrap();
        }
        drop(w);

        for i in 0..5 {
            let received: Option<ClientMessage> = read_message(&mut r).await.unwrap();
            match received.unwrap() {
                ClientMessage::PaneInput { pane_id, data } => {
                    assert_eq!(pane_id, i);
                    assert_eq!(data, vec![b'a' + i as u8]);
                }
                _ => panic!("wrong variant"),
            }
        }

        // Should return None at EOF
        let end: Option<ClientMessage> = read_message(&mut r).await.unwrap();
        assert!(end.is_none());
    }

    #[tokio::test]
    async fn eof_returns_none() {
        let (w, mut r) = tokio::io::duplex(64);
        drop(w);
        let result: Option<ClientMessage> = read_message(&mut r).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn large_message() {
        let (mut w, mut r) = tokio::io::duplex(1024 * 1024);

        let msg = ClientMessage::PaneInput {
            pane_id: 0,
            data: vec![0xAA; 100_000],
        };
        write_message(&mut w, &msg).await.unwrap();
        drop(w);

        let received: Option<ClientMessage> = read_message(&mut r).await.unwrap();
        match received.unwrap() {
            ClientMessage::PaneInput { data, .. } => assert_eq!(data.len(), 100_000),
            _ => panic!("wrong variant"),
        }
    }
}
