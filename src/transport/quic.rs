use crate::transport::accept::call_on_accept_stream;
use crate::transport::base::{
    ClientConfig, ServerConfig, TransformClient, TransformServer, TransportConnection,
    TransportKind, TransportStream,
};
use crate::transport::cert::NoCertificateVerification;
use quinn::{ClientConfig as QuinnClientConfig, Endpoint, RecvStream, SendStream, VarInt};
use rustls::ClientConfig as RustlsClientConfig;
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::{self, ReadBuf};
use tokio::io::{AsyncRead, AsyncWrite};

pub struct QuinnStream {
    pub send: SendStream,
    pub recv: RecvStream,
}

impl AsyncWrite for QuinnStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = &mut *self;
        Pin::new(&mut this.send)
            .poll_write(cx, buf)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = &mut *self;
        Pin::new(&mut this.send)
            .poll_flush(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = &mut *self;
        Pin::new(&mut this.send)
            .poll_shutdown(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

impl AsyncRead for QuinnStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = &mut *self;
        Pin::new(&mut this.recv).poll_read(cx, buf)
    }
}

impl TransportStream for QuinnStream {}
pub struct QuinnConnection {
    pub conn: quinn::Connection,
}

#[async_trait::async_trait]
impl TransportConnection for QuinnConnection {
    fn kind(&self) -> TransportKind {
        TransportKind::QUIC
    }
    async fn open_stream(&self) -> anyhow::Result<Box<dyn TransportStream>> {
        if let Some(reason) = self.conn.close_reason() {
            return Err(anyhow::anyhow!("Connection closed: {:?}", reason));
        }
        let (send, recv) = self.conn.open_bi().await?;
        Ok(Box::new(QuinnStream { send, recv }))
    }
}

pub struct QuinnServerEndpoint {}

#[async_trait::async_trait]
impl TransformServer for QuinnServerEndpoint {
    async fn bind(config: ServerConfig) -> Result<Arc<Self>, anyhow::Error>
    where
        Self: Sized,
    {
        crate::transport::cert::install_default_crypto_provider();

        println!("Loading certificate...");
        let (cert_der, key_der) =
            crate::transport::cert::load_cert(config.ssl_cert_path, config.ssl_key_path)?;
        let rustls_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .unwrap();
        let quic_server_config = quinn::crypto::rustls::QuicServerConfig::try_from(rustls_config)?;

        let mut transport_config = quinn::TransportConfig::default();
        transport_config.keep_alive_interval(Some(Duration::from_secs(20)));
        // transport_config.max_idle_timeout(Some(Duration::from_secs(120).try_into().unwrap()));

        let mut server_config =
            quinn::ServerConfig::with_crypto(std::sync::Arc::new(quic_server_config));
        server_config.transport = std::sync::Arc::new(transport_config);

        let endpoint = match Endpoint::server(server_config, config.addr.parse().unwrap()) {
            Ok(endpoint) => endpoint,
            Err(e) => {
                eprintln!("[QUIC Server] Failed to bind QUIC endpoint: {:?}", e);
                return Err(anyhow::anyhow!(e));
            }
        };

        println!("[QUIC Server] Endpoint started, waiting for new QUIC connections...");
        tokio::spawn(async move {
            while let Some(connecting) = endpoint.accept().await {
                if let Ok(conn) = connecting.await {
                    let conn_clone = conn.clone();
                    tokio::spawn(async move {
                        let connection = Arc::new(QuinnConnection {
                            conn: conn_clone.clone(),
                        });
                        while let Ok((send, recv)) = conn_clone.accept_bi().await {
                            let connection = connection.clone();
                            tokio::spawn(async move {
                                let _ = call_on_accept_stream(
                                    connection,
                                    Box::new(QuinnStream { send, recv }),
                                )
                                .await;
                            });
                        }
                    });
                }
            }
        });
        Ok(Arc::new(QuinnServerEndpoint {}))
    }
}

pub struct QuinnClientEndpoint {
    pub conn: Arc<QuinnConnection>,
}

#[async_trait::async_trait]
impl TransformClient for QuinnClientEndpoint {
    async fn connect(config: ClientConfig) -> Result<Arc<Self>, anyhow::Error> {
        crate::transport::cert::install_default_crypto_provider();

        let mut client_config = RustlsClientConfig::builder()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth();

        client_config
            .dangerous()
            .set_certificate_verifier(Arc::new(NoCertificateVerification));

        let quic_client_config =
            match quinn::crypto::rustls::QuicClientConfig::try_from(client_config) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Failed to create QUIC client config: {}", e);
                    return Err(anyhow::anyhow!(e));
                }
            };

        let mut transport_config = quinn::TransportConfig::default();
        transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
        // 与服务端保持一致：120秒超时
        // transport_config.max_idle_timeout(Some(
        //     std::time::Duration::from_secs(120).try_into().unwrap(),
        // ));

        let mut client_config = QuinnClientConfig::new(Arc::new(quic_client_config));
        client_config.transport_config(Arc::new(transport_config));

        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        endpoint.set_default_client_config(client_config);

        println!("Connecting to Server: {} ...", config.addr);
        let addr = match config.addr.parse() {
            Ok(addr) => addr,
            Err(e) => {
                eprintln!("Invalid server address: {}", e);
                return Err(anyhow::anyhow!("Invalid server address: {}", e));
            }
        };
        let connecting = endpoint.connect(addr, "localhost");
        let connecting = match connecting {
            Ok(connecting) => connecting,
            Err(e) => {
                eprintln!("Connection error: {}", e);
                return Err(anyhow::anyhow!(e));
            }
        };

        let conn = match connecting.await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Connection handshake error: {}", e);
                return Err(anyhow::anyhow!("Connection handshake error: {}", e));
            }
        };

        let conn_box = Arc::new(QuinnConnection { conn: conn.clone() });
        let conn_for_accept = conn_box.clone();
        tokio::spawn(async move {
            while let Ok(stream) = conn_for_accept.open_stream().await {
                let conn_for_callback = conn_for_accept.clone();
                tokio::spawn(async move {
                    let _ = call_on_accept_stream(conn_for_callback, stream).await;
                });
            }
        });

        Ok(Arc::new(QuinnClientEndpoint { conn: conn_box }))
    }

    async fn close(&self) -> anyhow::Result<()> {
        Ok(())
    }
}
