use crate::transport::base::{ServerConfig, TransformServer, TransportStream};
use crate::transport::quic::QuinnServerEndpoint;
use crate::tunnel::common::{AUTH_TOKEN_KEY, get_client_id_from_token};
use crate::tunnel::inbound::{InboundConfig, bind_tcp_inbound};
use crate::tunnel::outbound::forward_to_tcp;
use crate::tunnel::packet::{TunnelCommand, TunnelCommandPacket, TunnelMeta};
use crate::tunnel::session::{TRANSPORT_SESSION_MAP, TransportSession, clear_expired_sessions};
use serde_json::Value;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::io::WriteHalf;
use tokio::time::Instant;

pub async fn start_server(
    quic_bind_addr: String,
    tcp_bind_addr: String,
    cert_path: String,
    key_path: String,
) -> anyhow::Result<()> {
    println!(
        "[Supernode] Initializing with QUIC={} TCP={} cert={} key={}",
        quic_bind_addr, tcp_bind_addr, cert_path, key_path
    );
    let config = ServerConfig {
        addr: quic_bind_addr.clone(),
        ssl_cert_path: cert_path.clone(),
        ssl_key_path: key_path.clone(),
    };
    let inbound_config = InboundConfig {
        inbound_addr: tcp_bind_addr.clone(),
    };
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10 * 60)).await;
            println!("[Supernode] Clearing expired transport sessions...");
            clear_expired_sessions().await;
        }
    });

    tokio::select! {
        result = start_transport(config) => {
            if let Err(e) = result {
                eprintln!("[Supernode] Transport error: {:?}", e);
            }else{
                println!("[Supernode] Transport closed");
            }
        }
        result = bind_tcp_inbound(inbound_config) => {
            if let Err(e) = result {
                eprintln!("[Supernode] Inbound error: {:?}", e);
            }else{
                println!("[Supernode] Inbound closed");
            }
        }
    }
    Ok(())
}

async fn start_transport(config: ServerConfig) -> anyhow::Result<()> {
    let server = QuinnServerEndpoint::bind(config).await?;
    println!("[Supernode] QUIC server bound, waiting for streams...");
    server
        .accept(|conn_box, stream| async move {
            println!("[Supernode] Bi-directional QUIC stream accepted, waiting for command...");
            let (mut stream_reader, stream_writer) = tokio::io::split(stream);
            let packet = match TunnelCommandPacket::read_command1(&mut stream_reader).await {
                Ok(packet) => packet,
                Err(err) => {
                    eprintln!("[Supernode] Failed to read command packet: {:?}", err);
                    return Err(err);
                }
            };
            println!("[Supernode] Received command: {:?}", packet.command);
            match packet.command {
                TunnelCommand::Forward => {
                    println!("[Supernode] Forward command meta: {:?}", packet.meta);
                    if let Err(err) =
                        forward_to_tcp(stream_reader, stream_writer, packet, Option::None).await
                    {
                        eprintln!("[Supernode] forward_to_tcp failed: {:?}", err);
                        return Err(err);
                    }
                }
                TunnelCommand::Ping => {
                    let client_id = match packet.meta.get(AUTH_TOKEN_KEY) {
                        Some(token) => token.as_str().unwrap(),
                        None => "",
                    };
                    println!("[QUIC Server] Ping from client_id: {}", client_id);

                    if let Some(mut entry) = TRANSPORT_SESSION_MAP.get_mut(client_id) {
                        println!("[QUIC Server] Session found, updating ping_at");
                        entry.value_mut().ping_at = Instant::now();
                        if let Err(err) =
                            response_command(stream_writer, TunnelCommand::Pong, &packet.meta).await
                        {
                            eprintln!("[Supernode] Failed to respond Pong: {:?}", err);
                            return Err(err);
                        }
                    } else {
                        eprintln!(
                            "[QUIC Server] Session not found for client_id: {}",
                            client_id
                        );
                    }
                }
                TunnelCommand::Auth => {
                    let mut meta = TunnelMeta::from([("result".to_string(), Value::Bool(false))]);
                    let token: Option<&Value> = packet.meta.get(AUTH_TOKEN_KEY);
                    if let Some(token) = token {
                        if let Some(token_str) = token.as_str() {
                            let client_id = get_client_id_from_token(token_str);
                            TRANSPORT_SESSION_MAP.insert(
                                client_id,
                                TransportSession {
                                    conn: conn_box.clone(),
                                    meta: packet.meta.clone(),
                                    ping_at: Instant::now(),
                                },
                            );
                            meta = TunnelMeta::from([("result".to_string(), Value::Bool(true))]);
                        }
                    }
                    if let Err(err) =
                        response_command(stream_writer, TunnelCommand::AuthResult, &meta).await
                    {
                        eprintln!("[Supernode] Failed to respond AuthResult: {:?}", err);
                        return Err(err);
                    }
                }
                _ => {
                    eprintln!("Unsupported command: {:?}", packet.command);
                }
            }

            Ok(())
        })
        .await?;
    Ok(())
}

pub async fn response_command(
    mut stream: WriteHalf<Box<dyn TransportStream>>,
    command: TunnelCommand,
    meta: &TunnelMeta,
) -> Result<TunnelCommandPacket, anyhow::Error> {
    let command_packet = TunnelCommandPacket::new(command, meta);
    let command_bytes = command_packet.to_bytes();
    if let Err(e) = stream.write_all(&command_bytes).await {
        return Err(anyhow::anyhow!(e));
    }
    if let Err(e) = stream.flush().await {
        return Err(anyhow::anyhow!(e));
    }
    if let Err(e) = stream.shutdown().await {
        return Err(anyhow::anyhow!(e));
    }
    println!("response_command: {:?}", command_packet);
    Ok(command_packet)
}
