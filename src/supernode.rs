use ping_tunnel::lib::common::{AUTH_TOKEN_KEY, FORWARD_TO_KEY, get_client_id_from_token};
use ping_tunnel::lib::connections::CONNECTIONS;
use ping_tunnel::lib::forward::tcp_to_quic;
use ping_tunnel::lib::sniff::sniff_tcp;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::env;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 5 {
        eprintln!(
            "Usage: {} <quic_bind_addr:port> <tcp_bind_addr:port> <cert_path> <key_path>",
            args[0]
        );
        std::process::exit(1);
    }

    let quic_bind_addr = args[1].clone();
    let tcp_bind_addr = args[2].clone();
    let cert_path = args[3].clone();
    let key_path = args[4].clone();
    supernode_handle(quic_bind_addr, tcp_bind_addr, cert_path, key_path).await
}

async fn supernode_handle(
    quic_bind_addr: String,
    tcp_bind_addr: String,
    cert_path: String,
    key_path: String,
) -> anyhow::Result<()> {
    let (cert_der, key_der) = ping_tunnel::lib::cert::load_cert(cert_path, key_path)?;
    println!("Starting QUIC server on {}...", quic_bind_addr);
    ping_tunnel::lib::server::start_server(cert_der, key_der, quic_bind_addr.parse()?).await?;
    println!("QUIC server started successfully");

    println!("Binding TCP listener on {}...", tcp_bind_addr);
    let listener = TcpListener::bind(&tcp_bind_addr).await?;
    println!(
        "HTTP server listening on http://{}, forwarding to QUIC",
        tcp_bind_addr
    );
    println!("Server ready, waiting for connections...");

    loop {
        match listener.accept().await {
            Ok((tcp_stream, _peer_addr)) => {
                tokio::task::spawn(async move {
                    let tcp_stream = tcp_stream;
                    let (mut tcp_reader, mut tcp_writer) = tcp_stream.into_split();
                    let request_info = match sniff_tcp(&mut tcp_reader).await {
                        Ok(result) => result,
                        Err(e) => {
                            eprintln!("[ERROR] Unhandled TCP connection: {}", e);
                            return;
                        }
                    };
                    if request_info.method == "GET" && request_info.url == "/__internal__/clients" {
                        let clients = CONNECTIONS
                            .iter()
                            .filter(|k| k.value().ping_at.elapsed().as_secs() < 30)
                            .map(|k| k.value().meta.clone())
                            .collect::<Vec<HashMap<String, Value>>>();
                        let body = json!({
                         "status": "ok",
                         "clients": clients,
                         "count": clients.len()
                        });
                        write_response(&mut tcp_writer, &body)
                            .await
                            .unwrap_or_else(|e| {
                                eprintln!("[ERROR] Failed to write response: {}", e)
                            });
                        return;
                    }
                    const DEFAULT_TOKEN: &str = "my-secret-token";
                    let token = request_info
                        .headers
                        .get(AUTH_TOKEN_KEY)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| DEFAULT_TOKEN.to_string());

                    let client_id = get_client_id_from_token(&token);

                    let forward_to = request_info
                        .headers
                        .get(FORWARD_TO_KEY)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "".to_string());

                    if let Some(quic_connection) = CONNECTIONS.get(&client_id) {
                        tcp_to_quic(
                            tcp_reader,
                            tcp_writer,
                            quic_connection.value().conn.clone(),
                            forward_to.clone(),
                        )
                        .await;
                    } else {
                        let body = json!({
                            "status": "error",
                            "message": format!("找不到客户端: {}", client_id)
                        });
                        write_response(&mut tcp_writer, &body)
                            .await
                            .unwrap_or_else(|e| {
                                eprintln!("[ERROR] Failed to write response: {}", e)
                            });
                    }
                });
            }
            Err(e) => {
                eprintln!("Error accepting TCP connection: {}", e);
                continue;
            }
        }
    }
}

async fn write_response(
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
