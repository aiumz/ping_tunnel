use crate::lib::cert::get_client_crypto_config;
use crate::lib::common::{AUTH_TOKEN_KEY, FORWARD_TO_KEY};
use crate::lib::connections::{CONNECTIONS, ConnectionSession, DEFAULT_CLIENT_ID};
use crate::lib::forward::{command_to_quic, quic_to_tcp};
use crate::lib::packet::{TunnelCommand, TunnelCommandPacket, TunnelMeta};
use quinn::{ClientConfig, Endpoint};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

pub async fn connect_to_server(server_addr: String, token: String, forward_to: String) {
    let client_crypto_config = match get_client_crypto_config() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to create QUIC client config: {}", e);
            return;
        }
    };
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
    transport_config.max_idle_timeout(None);

    let mut client_config = ClientConfig::new(Arc::new(client_crypto_config));
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
        CONNECTIONS.insert(
            DEFAULT_CLIENT_ID.to_string(),
            ConnectionSession::new(conn.clone()),
        );
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
