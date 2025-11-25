use crate::lib::common::{AUTH_TOKEN_KEY, FORWARD_TO_KEY, get_client_id_from_token};
use crate::lib::connections::{CONNECTIONS, ConnectionSession};
use crate::lib::forward::{quic_to_tcp, response_command};
use crate::lib::packet::{TunnelCommand, TunnelCommandPacket, TunnelMeta};
use quinn::{Endpoint, ServerConfig};
use rustls::ServerConfig as RustlsServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use serde_json::Value;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub async fn start_server(
    cert_der: CertificateDer<'static>,
    key_der: PrivateKeyDer<'static>,
    bind_addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    let rustls_config = RustlsServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .unwrap();

    let quic_server_config = quinn::crypto::rustls::QuicServerConfig::try_from(rustls_config)?;

    let mut transport_config = quinn::TransportConfig::default();
    transport_config.keep_alive_interval(Some(Duration::from_secs(10)));
    transport_config.max_idle_timeout(None);

    let mut server_config = ServerConfig::with_crypto(Arc::new(quic_server_config));
    server_config.transport = Arc::new(transport_config);

    let endpoint = Endpoint::server(server_config, bind_addr)?;

    {
        let endpoint = endpoint.clone();
        tokio::spawn(async move {
            println!("QUIC server started, waiting for connections on 0.0.0.0:4433");
            while let Some(connecting) = endpoint.accept().await {
                tokio::spawn(async move {
                    match connecting.await {
                        Ok(conn) => loop {
                            match conn.accept_bi().await {
                                Ok((send, recv)) => {
                                    let conn_clone = conn.clone();
                                    tokio::spawn(async move {
                                        tunnel_handle(send, recv, conn_clone).await;
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
        });
    }

    {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10 * 60)).await;
                CONNECTIONS
                    .retain(|_, session| session.ping_at.elapsed() < Duration::from_secs(60));
            }
        });
    }
    Ok(())
}

async fn tunnel_handle(
    send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    conn: quinn::Connection,
) {
    let connections = CONNECTIONS.clone();
    let header = match TunnelCommandPacket::read_command(&mut recv).await {
        Ok(result) => result,
        Err(e) => {
            eprintln!(
                "[ERROR] Failed to read tunnel header: {}, connection state: {:?}",
                e,
                conn.close_reason()
            );
            return;
        }
    };
    let token = match header.meta.get(AUTH_TOKEN_KEY) {
        Some(token) => token.as_str().unwrap_or_default(),
        None => {
            eprintln!("[ERROR] Empty token received in Ping command!");
            return;
        }
    };

    let client_id = get_client_id_from_token(&token);
    match header.command {
        TunnelCommand::Auth => {
            let auth_response_meta = TunnelMeta::from([("result".to_string(), Value::Bool(true))]);
            let auth_response =
                TunnelCommandPacket::new(TunnelCommand::AuthResult, &auth_response_meta);
            if let Err(e) = response_command(send, auth_response).await {
                eprintln!("[ERROR] Failed to send auth response: {}", e);
                return;
            } else {
                connections.insert(client_id.clone(), ConnectionSession::new(conn.clone()));
                println!("[INFO] Client {} authenticated successfully", client_id);
            }
        }
        TunnelCommand::Ping => {
            if let Some(mut session) = connections.get_mut(&client_id) {
                session.ping_at = Instant::now();
                let ping_response_meta = TunnelMeta::new();
                let ping_response =
                    TunnelCommandPacket::new(TunnelCommand::Pong, &ping_response_meta);
                if let Err(e) = response_command(send, ping_response).await {
                    eprintln!("[ERROR] Failed to send ping response: {}", e);
                    return;
                }
            } else {
                eprintln!("[ERROR] Client not found: {}", client_id);
                return;
            }
        }
        TunnelCommand::Forward => {
            let forward_to = match header.meta.get(FORWARD_TO_KEY).and_then(|v| v.as_str()) {
                Some(forward_to) => forward_to,
                None => {
                    eprintln!("[ERROR] Empty forward_to received in Forward command!");
                    return;
                }
            };
            quic_to_tcp(forward_to, recv, send).await;
        }
        _ => {
            eprintln!("Invalid command: {:?}", header.command);
        }
    }
}
