use serde_json::{Value, json};
use std::sync::{Arc, LazyLock};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use crate::tunnel::{
    common::FORWARD_TO_KEY,
    packet::{TunnelCommand, TunnelCommandPacket, TunnelMeta},
    session::{TRANSPORT_SESSION_MAP, get_default_session, get_session},
    sniff,
};

pub static TCP_INBOUND_ADDR: LazyLock<Arc<RwLock<String>>> =
    LazyLock::new(|| Arc::new(RwLock::new(String::new())));

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
    if let Ok(addr) = listener.local_addr() {
        *TCP_INBOUND_ADDR.write().await = addr.to_string();
        println!("tcp inbound addr: {}", TCP_INBOUND_ADDR.read().await);
    }

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
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
                    let tunnel_id = request_info.tunnel_id.clone();
                    let session = get_default_session().or_else(|| get_session(&tunnel_id));
                    println!("session: {:?}", session.is_some());
                    if let Some(session) = session {
                        if session.ping_at.elapsed().as_secs() > 60 {
                            eprintln!("session timeout, will remove session");
                            TRANSPORT_SESSION_MAP.remove(&tunnel_id);
                            return;
                        }
                        let upstream_stream = match session.conn.open_stream().await {
                            Ok(stream) => stream,
                            Err(e) => {
                                eprintln!("open_stream error: {:?}", e);
                                TRANSPORT_SESSION_MAP.remove(&tunnel_id);
                                return;
                            }
                        };
                        let (mut upstream_reader, mut upstream_writer) =
                            tokio::io::split(upstream_stream);

                        println!("Forwarding HTTP request to: {}", request_info.host);

                        let tcp_to_transport = tokio::spawn(async move {
                            let meta = TunnelMeta::from([(
                                FORWARD_TO_KEY.to_string(),
                                Value::String(request_info.host.clone()),
                            )]);
                            let command = TunnelCommandPacket::new(TunnelCommand::Forward, &meta);
                            println!("Sending Forward command: {:?}", command);
                            if let Err(e) = upstream_writer.write_all(&command.to_bytes()).await {
                                eprintln!("Failed to send Forward command: {:?}", e);
                                return;
                            }
                            if let Err(e) =
                                tokio::io::copy(&mut tcp_recv, &mut upstream_writer).await
                            {
                                eprintln!("copy stream -> upstream error: {:?}", e);
                            }
                            upstream_writer.shutdown().await.ok();
                        });
                        let transport_to_tcp = tokio::spawn(async move {
                            if let Err(e) =
                                tokio::io::copy(&mut upstream_reader, &mut tcp_send).await
                            {
                                eprintln!("copy upstream -> stream error: {:?}", e);
                            }
                            tcp_send.shutdown().await.ok();
                        });
                        let res = tokio::try_join!(tcp_to_transport, transport_to_tcp);
                        if let Err(e) = res {
                            eprintln!("copy stream -> upstream error: {:?}", e);
                        };
                    } else {
                        let _ = json_response(
                            &mut tcp_send,
                            &json!({
                                "code": 404,
                                "message": format!("tunnel [{}] not online", tunnel_id),
                            }),
                        )
                        .await;
                    }
                });
            }
            Err(e) => {
                eprintln!("accept error: {:?}", e);
                continue;
            }
        }
    }
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
        .shutdown()
        .await
        .unwrap_or_else(|e| eprintln!("[ERROR] Failed to shutdown TCP client: {}", e));
    Ok(())
}
