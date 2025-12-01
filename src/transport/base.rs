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

    async fn accept<'a, F, Fut>(&'a self, callback: F) -> Result<(), anyhow::Error>
    where
        F: Fn(
                Arc<dyn TransportConnection + Send + Sync + 'static>,
                Box<dyn TransportStream>,
            ) -> Fut
            + Send
            + Sync
            + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static;
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

    async fn accept<F, Fut>(&self, callback: F) -> Result<(), anyhow::Error>
    where
        F: Fn(Box<dyn TransportStream>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static;

    async fn close(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    fn get_conn(&self) -> Arc<dyn TransportConnection + Send + Sync + 'static>;
}

#[async_trait::async_trait]
pub trait Outbound: Send + Sync {
    async fn run(&mut self, stream: Box<dyn TransportStream>) -> anyhow::Result<()>;
}
