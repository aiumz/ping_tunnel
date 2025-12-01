use crate::transport::base::{ClientConfig, TransformClient};
use crate::transport::quic::QuinnClientEndpoint;
use crate::tunnel::common::AUTH_TOKEN_KEY;
use crate::tunnel::inbound::{InboundConfig, bind_tcp_inbound};
use crate::tunnel::outbound::forward_to_tcp;
use crate::tunnel::packet::{TunnelCommand, TunnelCommandPacket, TunnelMeta};
use crate::tunnel::session::DEFAULT_CLIENT_ID;
use crate::tunnel::session::{TRANSPORT_SESSION_MAP, TransportSession, get_default_session};
use serde_json::Value;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

pub async fn start_client(
    server_addr: String,
    token: String,
    forward_to: String,
) -> anyhow::Result<()> {
    tokio::select! {
        result = start_transport(server_addr, token, forward_to) => {
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

async fn start_transport(
    server_addr: String,
    token: String,
    forward_to: String,
) -> anyhow::Result<()> {
    let config = ClientConfig {
        addr: server_addr.clone(),
    };
    println!("Connecting to server...");
    let mut is_connected = false;
    const SLEEP_TIME: Duration = Duration::from_secs(10);
    let meta = TunnelMeta::from([(AUTH_TOKEN_KEY.to_string(), Value::String(token.clone()))]);
    loop {
        if !is_connected {
            let config = config.clone();
            let client = match QuinnClientEndpoint::connect(config).await {
                Ok(client) => client,
                Err(e) => {
                    eprintln!(
                        "Failed to connect to server: {:?}, retrying in seconds...",
                        e
                    );
                    tokio::time::sleep(SLEEP_TIME).await;
                    continue;
                }
            };
            TRANSPORT_SESSION_MAP.insert(
                DEFAULT_CLIENT_ID.to_string(),
                TransportSession {
                    conn: client.get_conn(),
                    meta: std::collections::HashMap::new(),
                    ping_at: tokio::time::Instant::now(),
                },
            );
            println!("Connected successfully!");

            match send_command(TunnelCommand::Auth, &meta).await {
                Ok(response) => {
                    if response.meta.get("result").unwrap().as_bool().unwrap() {
                        println!("Auth successful");
                    } else {
                        eprintln!("Auth failed: invalid response");
                        TRANSPORT_SESSION_MAP.remove(DEFAULT_CLIENT_ID);
                        tokio::time::sleep(SLEEP_TIME).await;
                        continue;
                    }
                }
                Err(e) => {
                    eprintln!("Auth failed: {}", e);
                    TRANSPORT_SESSION_MAP.remove(DEFAULT_CLIENT_ID);
                    tokio::time::sleep(SLEEP_TIME).await;
                    continue;
                }
            }
            {
                let forward_to = forward_to.clone();
                let client_for_accept = client.clone();
                tokio::spawn(async move {
                    println!("[QUIC Client] Starting accept loop to receive server streams...");
                    if let Err(e) = client_for_accept
                        .accept(move |stream| {
                            let forward_to = forward_to.clone();
                            async move {
                                let (mut stream_reader, stream_writer) = tokio::io::split(stream);
                                let packet =
                                    TunnelCommandPacket::read_command1(&mut stream_reader).await?;
                                println!("[QUIC Client] Received command: {:?}", packet);
                                match packet.command {
                                    TunnelCommand::Forward => {
                                        forward_to_tcp(
                                            stream_reader,
                                            stream_writer,
                                            packet,
                                            Some(forward_to),
                                        )
                                        .await?;
                                    }
                                    _ => {
                                        eprintln!(
                                            "[QUIC Client] Unsupported command: {:?}",
                                            packet.command
                                        );
                                    }
                                }
                                Ok(())
                            }
                        })
                        .await
                    {
                        eprintln!("[QUIC Client] Accept error: {:?}", e);
                    }
                });
            }
        }

        match send_command(TunnelCommand::Ping, &meta).await {
            Ok(response) => {
                if response.command as u8 == TunnelCommand::Pong as u8 {
                    println!("Ping successful");
                    is_connected = true;
                } else {
                    eprintln!("Ping failed: invalid response");
                    is_connected = false;
                    TRANSPORT_SESSION_MAP.remove(DEFAULT_CLIENT_ID);
                }
            }
            Err(e) => {
                eprintln!("Ping failed: {}", e);
                is_connected = false;
                TRANSPORT_SESSION_MAP.remove(DEFAULT_CLIENT_ID);
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
    let session = get_default_session();
    if let Some(session) = session {
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
