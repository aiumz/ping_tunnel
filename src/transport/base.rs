use anyhow::Result;
use std::future::Future;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
pub trait TransportStream: AsyncWrite + AsyncRead + Unpin + Send + Sync {}

pub enum TransportKind {
    QUIC,
}
#[async_trait::async_trait]
pub trait TransportConnection: Send + Sync {
    fn kind(&self) -> TransportKind;
    async fn open_stream(&self) -> anyhow::Result<Box<dyn TransportStream>>;
}

pub struct ServerConfig {
    pub addr: String,
    pub ssl_cert_path: String,
    pub ssl_key_path: String,
}

#[async_trait::async_trait]
pub trait TransformServer: Send + Sync {
    async fn bind(config: ServerConfig) -> Result<Arc<Self>, anyhow::Error>
    where
        Self: Sized;
}

#[derive(Clone)]
pub struct ClientConfig {
    pub addr: String,
}
#[async_trait::async_trait]
pub trait TransformClient: Send + Sync + std::any::Any {
    async fn connect(config: ClientConfig) -> Result<Arc<Self>, anyhow::Error>
    where
        Self: Sized;

    async fn close(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait Outbound: Send + Sync {
    async fn run(&mut self, stream: Box<dyn TransportStream>) -> anyhow::Result<()>;
}
