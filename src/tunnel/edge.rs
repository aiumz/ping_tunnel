use crate::transport::accept::register_on_accept_stream;
use crate::transport::base::{ClientConfig, TransformClient};
use crate::transport::quic::QuinnClientEndpoint;
use crate::tunnel::common::{AUTH_TOKEN_KEY, resolve_socket_addr};
use crate::tunnel::inbound::{InboundConfig, bind_tcp_inbound};
use crate::tunnel::outbound::tcp_outbound;
use crate::tunnel::packet::{TunnelCommand, TunnelMeta, TunnelPacket, send_packet};
use crate::tunnel::session::{DEFAULT_CLIENT_ID, refresh_session_by_id};
use crate::tunnel::session::{TransportSession, insert_session, remove_session};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

pub async fn start_client(
    server_addr: String,
    token: String,
    forward_to: String,
) -> anyhow::Result<()> {
    let server_addr = resolve_socket_addr(&server_addr)?;

    let forward_to = Arc::new(forward_to);
    register_on_accept_stream(move |_conn, stream| {
        let forward_to = forward_to.clone();
        async move {
            let (mut stream_reader, stream_writer) = tokio::io::split(stream);
            let packet = TunnelPacket::read_command(&mut stream_reader).await?;
            match packet.command {
                TunnelCommand::Forward => {
                    tcp_outbound(
                        stream_reader,
                        stream_writer,
                        packet,
                        Some(forward_to.to_string()),
                    )
                    .await?;
                }
                _ => {
                    warn!("Unsupported command: {:?}", packet.command);
                }
            }
            Ok(())
        }
    });

    tokio::select! {
        result = start_transport(server_addr, token) => {
            if let Err(e) = result {
                error!("Transport error: {:?}", e);
            }
        }
        result = bind_tcp_inbound(InboundConfig {
            inbound_addr: "127.0.0.1:0".to_string(),
        }, true) => {
            if let Err(e) = result {
                error!("Inbound error: {:?}", e);
            }
        }
    }
    Ok(())
}

async fn start_transport(server_addr: String, token: String) -> anyhow::Result<()> {
    let config = ClientConfig {
        addr: server_addr.clone(),
    };
    info!("Connecting to server...");
    let mut is_connected = false;
    const SLEEP_TIME: Duration = Duration::from_secs(10);
    let meta = TunnelMeta::from([(AUTH_TOKEN_KEY.to_string(), Value::String(token.clone()))]);
    loop {
        if !is_connected {
            let config = config.clone();
            if let Ok(client) = QuinnClientEndpoint::connect(config).await {
                info!("Connected successfully!");
                client.conn.set_client_id(DEFAULT_CLIENT_ID.to_string());
                insert_session(
                    DEFAULT_CLIENT_ID.to_string(),
                    TransportSession {
                        conn: client.conn.clone(),
                        meta: std::collections::HashMap::new(),
                    },
                )
                .await;
                if let Ok(response) = send_packet(TunnelCommand::Auth, &meta).await {
                    if response.meta.get("result").unwrap().as_bool().unwrap() {
                        is_connected = true;
                    } else {
                        error!("Auth failed: invalid response");
                        is_connected = false;
                        remove_session(DEFAULT_CLIENT_ID).await;
                        tokio::time::sleep(SLEEP_TIME).await;
                        continue;
                    }
                }
            } else {
                tokio::time::sleep(SLEEP_TIME).await;
                continue;
            }
        } else {
            if let Ok(response) = send_packet(TunnelCommand::Ping, &meta).await {
                if response.command as u8 == TunnelCommand::Pong as u8 {
                    is_connected = true;
                    refresh_session_by_id(DEFAULT_CLIENT_ID).await;
                } else {
                    warn!("Ping failed: invalid response");
                    is_connected = false;
                    remove_session(DEFAULT_CLIENT_ID).await;
                }
            } else {
                is_connected = false;
                remove_session(DEFAULT_CLIENT_ID).await;
                tokio::time::sleep(SLEEP_TIME).await;
                continue;
            }
        }

        tokio::time::sleep(SLEEP_TIME).await;
        continue;
    }
}
