use crate::transport::base::{
    ClientConfig, ServerConfig, TransformClient, TransformServer, TransportConnection,
    TransportKind, TransportStream,
};
use crate::transport::cert::NoCertificateVerification;
use quinn::{ClientConfig as QuinnClientConfig, Endpoint, RecvStream, SendStream};
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
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let send = unsafe { self.map_unchecked_mut(|s| &mut s.send) };
        send.poll_write(cx, buf)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), io::Error>> {
        let send = unsafe { self.map_unchecked_mut(|s| &mut s.send) };
        send.poll_flush(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), io::Error>> {
        let send = unsafe { self.map_unchecked_mut(|s| &mut s.send) };
        send.poll_shutdown(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }
}
impl AsyncRead for QuinnStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let recv = unsafe { self.map_unchecked_mut(|s| &mut s.recv) };
        recv.poll_read(cx, buf)
    }
}
impl TransportStream for QuinnStream {}
pub struct QuinnConnection {
    pub conn: Arc<quinn::Connection>,
}

#[async_trait::async_trait]
impl TransportConnection for QuinnConnection {
    fn kind(&self) -> TransportKind {
        TransportKind::QUIC
    }
    async fn open_stream(&self) -> anyhow::Result<Box<dyn TransportStream>> {
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
        transport_config.max_idle_timeout(None);

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
            while let Some(connecting) = endpoint.accept().await {
                let callback = callback.clone();
                tokio::spawn(async move {
                    let callback = callback.clone();
                    match connecting.await {
                        Ok(conn) => loop {
                            let conn_box = Arc::new(QuinnConnection {
                                conn: Arc::new(conn.clone()),
                            });
                            match conn.accept_bi().await {
                                Ok((send, recv)) => {
                                    let callback = callback.clone();
                                    tokio::spawn(async move {
                                        let _ = callback(
                                            conn_box,
                                            Box::new(QuinnStream { send, recv }),
                                        )
                                        .await;
                                    });
                                }
                                Err(e) => {
                                    eprintln!("[ERROR] accept_bi failed: {:?}", e);
                                    break;
                                }
                            }
                        },
                        Err(e) => {
                            eprintln!("[ERROR] Connection failed: {:?}", e);
                            return;
                        }
                    }
                });
            }
            Ok(())
        } else {
            Err(anyhow::anyhow!("Endpoint not found"))
        }
    }
}

pub struct QuinnClientEndpoint {
    pub conn: Arc<quinn::Connection>,
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
        transport_config
            .max_idle_timeout(Some(std::time::Duration::from_secs(30).try_into().unwrap()));

        let mut client_config = QuinnClientConfig::new(Arc::new(quic_client_config));
        client_config.transport_config(Arc::new(transport_config));

        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        endpoint.set_default_client_config(client_config);

        loop {
            println!("Connecting to Server: {} ...", config.addr);
            let conn = match tokio::time::timeout(Duration::from_secs(10), async {
                let connecting = endpoint.connect(config.addr.parse().unwrap(), "localhost");
                let connecting = match connecting {
                    Ok(connecting) => connecting,
                    Err(e) => return Err(anyhow::anyhow!(e)),
                };
                match connecting.await {
                    Ok(conn) => Ok(conn),
                    Err(e) => return Err(anyhow::anyhow!(e)),
                }
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
                Err(_e) => {
                    eprintln!("Connection timeout, retrying in 2 seconds... ");
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    continue;
                }
            };
            return Ok(Arc::new(QuinnClientEndpoint {
                conn: Arc::new(conn),
            }));
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
