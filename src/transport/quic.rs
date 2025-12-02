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
        // 检查连接是否已关闭
        if let Some(reason) = self.conn.close_reason() {
            return Err(anyhow::anyhow!("Connection closed: {:?}", reason));
        }
        let (send, recv) = self.conn.open_bi().await?;
        Ok(Box::new(QuinnStream { send, recv }))
    }
}

pub struct QuinnServerEndpoint {
    pub endpoint: Option<Endpoint>,
}

impl QuinnServerEndpoint {
    pub fn new() -> Self {
        Self { endpoint: None }
    }
}

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
        transport_config.keep_alive_interval(Some(Duration::from_secs(10)));
        transport_config.max_idle_timeout(Some(Duration::from_secs(120).try_into().unwrap()));

        let mut server_config =
            quinn::ServerConfig::with_crypto(std::sync::Arc::new(quic_server_config));
        server_config.transport = std::sync::Arc::new(transport_config);
        let endpoint = Endpoint::server(server_config, config.addr.parse().unwrap()).unwrap();
        Ok(Arc::new(QuinnServerEndpoint {
            endpoint: Some(endpoint),
        }))
    }
    async fn accept<'a, F, Fut>(&'a self, callback: F) -> Result<(), anyhow::Error>
    where
        F: Fn(
                Arc<dyn TransportConnection + Send + Sync + 'static>,
                Box<dyn TransportStream>,
            ) -> Fut
            + Send
            + Sync
            + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        let callback = Arc::new(callback);
        if let Some(endpoint) = self.endpoint.as_ref() {
            println!("[QUIC Server] Endpoint started, waiting for new QUIC connections...");
            loop {
                // 使用 loop + match 而不是 while let，以便更好地处理 None 情况
                match endpoint.accept().await {
                    Some(connecting) => {
                        let callback = callback.clone();
                        tokio::spawn(async move {
                            let callback = callback.clone();
                            println!(
                                "[QUIC Server] Incoming QUIC connection, waiting for handshake..."
                            );
                            match connecting.await {
                                Ok(conn) => {
                                    let conn_box = Arc::new(QuinnConnection { conn: conn.clone() });
                                    loop {
                                        let conn_for_accept = conn.clone();
                                        match conn_for_accept.accept_bi().await {
                                            Ok((send, recv)) => {
                                                let remote = conn_for_accept.remote_address();
                                                println!(
                                                    "[QUIC Server] accept_bi ok: new bi-stream from {}",
                                                    remote
                                                );
                                                let callback = callback.clone();
                                                let conn_for_callback = conn_box.clone();
                                                tokio::spawn(async move {
                                                    let _ = callback(
                                                        conn_for_callback,
                                                        Box::new(QuinnStream { send, recv }),
                                                    )
                                                    .await;
                                                });
                                            }
                                            Err(e) => {
                                                eprintln!(
                                                    "[QUIC Server] accept_bi failed: {:?}",
                                                    e
                                                );
                                                break;
                                            }
                                        }
                                    }
                                    conn.close(VarInt::from(0u32), b"");
                                    println!(
                                        "[QUIC Server] Stream accept loop ended for connection"
                                    );
                                }
                                Err(e) => {
                                    eprintln!("[ERROR] Connection handshake failed: {:?}", e);
                                    // 不 return，让外层循环继续等待新连接
                                }
                            }
                        });
                    }
                    None => {
                        eprintln!(
                            "[QUIC Server] endpoint.accept() returned None, endpoint may be closed"
                        );
                        // 如果 endpoint 被关闭，退出循环
                        break;
                    }
                }
            }
            println!("[QUIC Server] Endpoint.accept() loop ended (no more incoming connections).");
            Ok(())
        } else {
            Err(anyhow::anyhow!("Endpoint not found"))
        }
    }
}

pub struct QuinnClientEndpoint {
    pub conn: quinn::Connection,
}

#[async_trait::async_trait]
impl TransformClient for QuinnClientEndpoint {
    async fn connect(config: ClientConfig) -> anyhow::Result<Arc<Self>> {
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
        transport_config.max_idle_timeout(Some(
            std::time::Duration::from_secs(120).try_into().unwrap(),
        ));

        let mut client_config = QuinnClientConfig::new(Arc::new(quic_client_config));
        client_config.transport_config(Arc::new(transport_config));

        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        endpoint.set_default_client_config(client_config);

        loop {
            println!("Connecting to Server: {} ...", config.addr);
            let addr = match config.addr.parse() {
                Ok(addr) => addr,
                Err(e) => {
                    eprintln!("Invalid server address: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    continue;
                }
            };
            let conn = match tokio::time::timeout(Duration::from_secs(10), async {
                let connecting = endpoint.connect(addr, "localhost");
                let connecting = match connecting {
                    Ok(connecting) => connecting,
                    Err(e) => {
                        eprintln!("Connection error: {}", e);
                        return Err(anyhow::anyhow!(e));
                    }
                };
                connecting.await.map_err(|e| {
                    eprintln!("Connection handshake error: {}", e);
                    anyhow::anyhow!(e)
                })
            })
            .await
            {
                Ok(Ok(conn)) => {
                    println!("Connected to Server successfully: {}", config.addr);
                    conn
                }
                Ok(Err(e)) => {
                    eprintln!("Failed to connect to Server: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    continue;
                }
                Err(e) => {
                    eprintln!(
                        "Connection timeout after 10s: {:?}, retrying in 2 seconds...",
                        e
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    continue;
                }
            };
            return Ok(Arc::new(QuinnClientEndpoint { conn }));
        }
    }

    async fn accept<'a, F, Fut>(&'a self, callback: F) -> anyhow::Result<()>
    where
        F: Fn(Box<dyn TransportStream>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        let conn = self.conn.clone();
        let callback = Arc::new(callback);
        tokio::spawn(async move {
            while let Ok((send, recv)) = conn.accept_bi().await {
                let callback = callback.clone();
                tokio::spawn(async move {
                    let _ = callback(Box::new(QuinnStream { send, recv })).await;
                });
            }
        })
        .await?;
        Ok(())
    }

    async fn close(&self) -> anyhow::Result<()> {
        return Ok(());
    }

    fn get_conn(&self) -> Arc<dyn TransportConnection + Send + Sync + 'static> {
        Arc::new(QuinnConnection {
            conn: self.conn.clone(),
        })
    }
}
