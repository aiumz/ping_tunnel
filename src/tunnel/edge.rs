use crate::transport::accept::register_on_accept_stream;
use crate::transport::base::{ClientConfig, TransformClient};
use crate::transport::quic::QuinnClientEndpoint;
use crate::tunnel::common::AUTH_TOKEN_KEY;
use crate::tunnel::inbound::{InboundConfig, bind_tcp_inbound};
use crate::tunnel::outbound::forward_to_tcp;
use crate::tunnel::packet::{TunnelCommand, TunnelCommandPacket, TunnelMeta};
use crate::tunnel::session::{DEFAULT_CLIENT_ID, refresh_session_by_id};
use crate::tunnel::session::{
    TransportSession, get_default_session, insert_session, remove_session,
};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

pub async fn start_client(
    server_addr: String,
    token: String,
    forward_to: String,
) -> anyhow::Result<()> {
    let forward_to = Arc::new(forward_to);
    register_on_accept_stream(move |_conn, stream| {
        let forward_to = forward_to.clone();
        async move {
            let (mut stream_reader, stream_writer) = tokio::io::split(stream);
            let packet = TunnelCommandPacket::read_command1(&mut stream_reader).await?;
            println!("[QUIC Client] Received command: {:?}", packet);
            match packet.command {
                TunnelCommand::Forward => {
                    forward_to_tcp(
                        stream_reader,
                        stream_writer,
                        packet,
                        Some(forward_to.to_string()),
                    )
                    .await?;
                }
                _ => {
                    eprintln!("[QUIC Client] Unsupported command: {:?}", packet.command);
                }
            }
            Ok(())
        }
    });

    tokio::select! {
        result = start_transport(server_addr, token) => {
            if let Err(e) = result {
                eprintln!("Transport error: {:?}", e);
            }
        }
        result = bind_tcp_inbound(InboundConfig {
            inbound_addr: "127.0.0.1:0".to_string(),
        }) => {
            if let Err(e) = result {
                eprintln!("Inbound error: {:?}", e);
            }
        }
    }
    Ok(())
}

async fn start_transport(server_addr: String, token: String) -> anyhow::Result<()> {
    let config = ClientConfig {
        addr: server_addr.clone(),
    };
    println!("Connecting to server...");
    let mut is_connected = false;
    const SLEEP_TIME: Duration = Duration::from_secs(10);
    let meta = TunnelMeta::from([(AUTH_TOKEN_KEY.to_string(), Value::String(token.clone()))]);
    loop {
        println!("Connecting to server... is_connected: {}", is_connected);
        if !is_connected {
            let config = config.clone();
            if let Ok(client) = QuinnClientEndpoint::connect(config).await {
                println!("Connected successfully!");
                insert_session(
                    DEFAULT_CLIENT_ID.to_string(),
                    TransportSession {
                        conn: client.conn.clone(),
                        meta: std::collections::HashMap::new(),
                    },
                )
                .await;
                if let Ok(response) = send_command(TunnelCommand::Auth, &meta).await {
                    if response.meta.get("result").unwrap().as_bool().unwrap() {
                        println!("Auth successful");
                        is_connected = true;
                    } else {
                        eprintln!("Auth failed: invalid response");
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
            if let Ok(response) = send_command(TunnelCommand::Ping, &meta).await {
                if response.command as u8 == TunnelCommand::Pong as u8 {
                    println!("Ping successful");
                    is_connected = true;
                    refresh_session_by_id(DEFAULT_CLIENT_ID).await;
                } else {
                    eprintln!("Ping failed: invalid response");
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

async fn send_command(
    command: TunnelCommand,
    meta: &TunnelMeta,
) -> Result<TunnelCommandPacket, anyhow::Error> {
    if let Some(session) = get_default_session().await {
        tokio::time::timeout(Duration::from_secs(10), async {
            let stream = session
                .conn
                .open_stream()
                .await
                .map_err(|e| anyhow::anyhow!("Connection closed: {}", e))?;
            let (mut recv_stream, mut send_stream) = tokio::io::split(stream);
            let command_packet = TunnelCommandPacket::new(command, meta);
            send_stream
                .write_all(&command_packet.to_bytes())
                .await
                .map_err(|e| anyhow::anyhow!("Write error: {}", e))?;
            send_stream
                .flush()
                .await
                .map_err(|e| anyhow::anyhow!("Flush error: {}", e))?;
            let response_packet = TunnelCommandPacket::read_command1(&mut recv_stream)
                .await
                .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;
            let _ = send_stream.shutdown().await;
            Ok(response_packet)
        })
        .await
        .map_err(|e| {
            let err_msg = e.to_string();
            if err_msg.contains("deadline has elapsed") || err_msg.contains("timeout") {
                anyhow::anyhow!("Command timeout")
            } else {
                anyhow::anyhow!("Command error: {}", e)
            }
        })?
    } else {
        Err(anyhow::anyhow!("Default connection not found"))
    }
}
