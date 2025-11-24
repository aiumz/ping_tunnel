use crate::lib::common::{AUTH_TOKEN_KEY, FORWARD_TO_KEY};
use crate::lib::forward::{command_to_quic, quic_to_tcp};
use crate::lib::packet::{TunnelCommand, TunnelCommandPacket, TunnelMeta};
use quinn::{ClientConfig, Endpoint};
use rustls::ClientConfig as RustlsClientConfig;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
struct NoCertificateVerification;

impl rustls::client::danger::ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
        ]
    }
}

pub type ClientConnection = Arc<quinn::Connection>;

pub async fn connect_to_server(server_addr: String, token: String, forward_to: String) {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install default crypto provider");

    let mut client_config = RustlsClientConfig::builder()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();

    client_config
        .dangerous()
        .set_certificate_verifier(Arc::new(NoCertificateVerification));

    let quic_client_config = match quinn::crypto::rustls::QuicClientConfig::try_from(client_config)
    {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to create QUIC client config: {}", e);
            return;
        }
    };

    let mut transport_config = quinn::TransportConfig::default();
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
    transport_config.max_idle_timeout(None);

    let mut client_config = ClientConfig::new(Arc::new(quic_client_config));
    client_config.transport_config(Arc::new(transport_config));

    let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
    endpoint.set_default_client_config(client_config);
    loop {
        println!("Connecting to Server: {} ...", server_addr);
        let conn = match tokio::time::timeout(Duration::from_secs(5), async {
            let connecting = endpoint.connect(server_addr.parse().unwrap(), "localhost");
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
                println!("Connected to Server successfully: {}", server_addr);
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

        {
            let token = token.clone();
            let conn = conn.clone();
            let auth_meta = TunnelMeta::from([(AUTH_TOKEN_KEY.to_string(), Value::String(token))]);
            let auth_packet = command_to_quic(conn, TunnelCommand::Auth, &auth_meta);
            match auth_packet.await {
                Ok(packet) => {
                    if packet.command as u8 == TunnelCommand::AuthResult as u8
                        && packet.meta.get("result").unwrap().as_bool().unwrap()
                    {
                    } else {
                        eprintln!("Invalid auth response: {:?}", packet.command);
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        continue;
                    }
                }
                Err(e) => {
                    eprintln!("Auth failed: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    continue;
                }
            }
        }

        let forward_task = {
            let conn = conn.clone();
            let forward_to = forward_to.to_string();
            tokio::spawn(async move {
                loop {
                    match conn.accept_bi().await {
                        Ok((send_stream, mut recv_stream)) => {
                            let cloned_forward_to = forward_to.to_string();
                            tokio::spawn(async move {
                                let recv_packet =
                                    TunnelCommandPacket::read_command(&mut recv_stream).await;
                                match recv_packet {
                                    Ok(packet) => match packet.command {
                                        TunnelCommand::Forward => {
                                            let final_forward_to = match packet
                                                .meta
                                                .get(FORWARD_TO_KEY)
                                                .and_then(|v| v.as_str())
                                            {
                                                Some(forward_to) => {
                                                    if forward_to.is_empty() {
                                                        cloned_forward_to.as_str()
                                                    } else {
                                                        forward_to
                                                    }
                                                }
                                                None => cloned_forward_to.as_str(),
                                            };

                                            quic_to_tcp(final_forward_to, recv_stream, send_stream)
                                                .await;
                                        }
                                        _ => {
                                            eprintln!("Invalid command: {:?}", packet.command);
                                        }
                                    },
                                    Err(e) => {
                                        eprintln!("Error reading tunnel header: {}", e);
                                        tokio::time::sleep(tokio::time::Duration::from_secs(2))
                                            .await;
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            eprintln!("Error accepting bidirectional stream: {}", e);
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        }
                    }
                }
            })
        };

        let ping_task: tokio::task::JoinHandle<Result<(), anyhow::Error>> = {
            let token = token.clone();
            let conn = conn.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    let meta = TunnelMeta::from([(
                        AUTH_TOKEN_KEY.to_string(),
                        Value::String(token.clone()),
                    )]);
                    let ping_packet = command_to_quic(conn.clone(), TunnelCommand::Ping, &meta);
                    match ping_packet.await {
                        Ok(packet) => {
                            if packet.command as u8 == TunnelCommand::Pong as u8 {
                            } else {
                                eprintln!("Invalid ping response: {:?}", packet.command);
                                return Err(anyhow::anyhow!("Invalid ping response"));
                            }
                        }
                        Err(e) => {
                            eprintln!("Ping failed: {}", e);
                            return Err(anyhow::anyhow!("Ping failed"));
                        }
                    }
                }
            })
        };

        tokio::select! {
            _ = forward_task => { },
            result = ping_task => {
                match result {
                    Ok(Ok(())) => {},
                    Ok(Err(e)) => {
                        eprintln!("Ping task failed: {}", e);
                    },
                    Err(e) => {
                        eprintln!("Ping task panicked: {}", e);
                    }
                }
            },
        }
    }
}
