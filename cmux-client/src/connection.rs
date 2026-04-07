use cmux_ipc::messages::{ClientMessage, ServerMessage};
use cmux_ipc::transport;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::windows::named_pipe::ClientOptions;

type PipeClient = tokio::net::windows::named_pipe::NamedPipeClient;

pub struct DaemonConnection {
    reader: ReadHalf<PipeClient>,
    writer: WriteHalf<PipeClient>,
}

impl DaemonConnection {
    pub async fn connect(pipe_name: &str) -> anyhow::Result<Self> {
        let client = ClientOptions::new().open(pipe_name)?;
        let (reader, writer) = tokio::io::split(client);
        Ok(Self { reader, writer })
    }

    pub async fn send(&mut self, msg: &ClientMessage) -> anyhow::Result<()> {
        transport::write_message(&mut self.writer, msg).await?;
        Ok(())
    }

    pub async fn recv(&mut self) -> anyhow::Result<Option<ServerMessage>> {
        let msg = transport::read_message(&mut self.reader).await?;
        Ok(msg)
    }

    pub fn split(self) -> (ReadHalf<PipeClient>, WriteHalf<PipeClient>) {
        (self.reader, self.writer)
    }
}
