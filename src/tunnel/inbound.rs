use std::{collections::HashMap, sync::Arc};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use crate::{
    tunnel::session::{TRANSPORT_SESSION_MAP, get_default_session, get_session},
    tunnel::{
        common::{AUTH_TOKEN_KEY, FORWARD_TO_KEY, get_client_id_from_token},
        packet::{TunnelCommand, TunnelCommandPacket, TunnelMeta},
        sniff,
    },
};
use serde_json::{Value, json};

pub struct InboundConfig {
    pub inbound_addr: String,
}
pub struct TcpInbound {
    pub listener: TcpListener,
}

pub async fn bind_tcp_inbound(config: InboundConfig) -> Result<Arc<TcpInbound>, anyhow::Error> {
    let listener = TcpListener::bind(config.inbound_addr.clone())
        .await
        .unwrap();
    println!("监听端口 {}，等待连接...", config.inbound_addr);
    while let Ok((stream, _addr)) = listener.accept().await {
        tokio::spawn(async move {
            let (mut tcp_recv, mut tcp_send) = stream.into_split();
            let request_info = match sniff::sniff_tcp(&mut tcp_recv).await {
                Ok(info) => info,
                Err(e) => {
                    eprintln!("sniff_tcp error: {:?}", e);
                    return;
                }
            };
            println!("request_info: {:?}", request_info);

            {
                if request_info.method == "GET" && request_info.url == "/__internal__/clients" {
                    let clients = TRANSPORT_SESSION_MAP
                        .iter()
                        .filter(|k| k.value().ping_at.elapsed().as_secs() < 30)
                        .map(|k| {
                            let meta = &k.value().meta;
                            meta.clone()
                        })
                        .collect::<Vec<HashMap<String, Value>>>();
                    let body = json!({
                        "status": "ok",
                        "clients": clients,
                        "count": clients.len()
                    });
                    if let Err(e) = json_response(&mut tcp_send, &body).await {
                        eprintln!("Failed to write response: {}", e);
                        return;
                    }
                    return;
                }
            }

            const DEFAULT_TOKEN: &str = "my-secret-token";
            let token = request_info
                .headers
                .get(AUTH_TOKEN_KEY)
                .map(|s| s.to_string())
                .unwrap_or_else(|| DEFAULT_TOKEN.to_string());

            let client_id = get_client_id_from_token(&token);

            println!("client_id: {:?}", client_id);
            let session = get_default_session().or_else(|| get_session(&client_id));
            println!("session: {:?}", session.is_some());
            if let Some(session) = session {
                let upstream_stream = match session.conn.open_stream().await {
                    Ok(stream) => stream,
                    Err(e) => {
                        eprintln!("open_stream error: {:?}", e);
                        TRANSPORT_SESSION_MAP.remove(&client_id);
                        return;
                    }
                };
                let (mut upstream_reader, mut upstream_writer) = tokio::io::split(upstream_stream);

                let forward_to = request_info
                    .headers
                    .get(FORWARD_TO_KEY)
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                println!("Forwarding HTTP request to: {}", forward_to);

                tokio::spawn(async move {
                    let mut meta = TunnelMeta::new();
                    if !forward_to.is_empty() {
                        meta.insert(
                            FORWARD_TO_KEY.to_string(),
                            Value::String(forward_to.clone()),
                        );
                    }
                    let command = TunnelCommandPacket::new(TunnelCommand::Forward, &meta);
                    println!("Sending Forward command: {:?}", command);
                    if let Err(e) = upstream_writer.write_all(&command.to_bytes()).await {
                        eprintln!("Failed to send Forward command: {:?}", e);
                        return;
                    }
                    if let Err(e) = upstream_writer.flush().await {
                        eprintln!("Failed to flush Forward command: {:?}", e);
                        return;
                    }

                    println!("Copying HTTP request data to QUIC stream...");
                    if let Err(e) = tokio::io::copy(&mut tcp_recv, &mut upstream_writer).await {
                        eprintln!("copy stream -> upstream error: {:?}", e);
                    }
                    if let Err(e) = upstream_writer.shutdown().await {
                        eprintln!("shutdown upstream_writer error: {:?}", e);
                    }
                });
                tokio::spawn(async move {
                    if let Err(e) = tokio::io::copy(&mut upstream_reader, &mut tcp_send).await {
                        eprintln!("copy upstream -> stream error: {:?}", e);
                    }
                    if let Err(e) = tcp_send.shutdown().await {
                        eprintln!("shutdown stream_writer error: {:?}", e);
                    }
                });
            }
        });
    }
    Ok(Arc::new(TcpInbound { listener }))
}

async fn json_response(
    tcp_writer: &mut tokio::net::tcp::OwnedWriteHalf,
    body: &Value,
) -> anyhow::Result<()> {
    let body_str = serde_json::to_string(body)?;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-cache\r\n\r\n{}",
        body_str.as_bytes().len(),
        body_str
    );
    tcp_writer
        .write_all(response.as_bytes())
        .await
        .unwrap_or_else(|e| eprintln!("[ERROR] Failed to write to TCP client: {}", e));
    tcp_writer
        .flush()
        .await
        .unwrap_or_else(|e| eprintln!("[ERROR] Failed to flush TCP client: {}", e));
    tcp_writer
        .shutdown()
        .await
        .unwrap_or_else(|e| eprintln!("[ERROR] Failed to shutdown TCP client: {}", e));
    Ok(())
}
