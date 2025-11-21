use hyper::Response;
use ping_tunnel::lib::common::{AUTH_TOKEN_KEY, FORWARD_TO_KEY, get_client_id_from_token};
use ping_tunnel::lib::connections::CONNECTIONS;
use ping_tunnel::lib::forward::tcp_to_quic;
use ping_tunnel::lib::sniff::sniff_tcp;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install default crypto provider");

    println!("Loading certificate...");
    let (cert_der, key_der) = ping_tunnel::lib::cert::load_cert()?;
    println!("Starting QUIC server on 0.0.0.0:4433...");
    ping_tunnel::lib::server::start_server(cert_der, key_der, "0.0.0.0:4433".parse()?).await?;
    println!("QUIC server started successfully");

    let addr: std::net::SocketAddr = ([0, 0, 0, 0], 4432).into();
    println!("Binding TCP listener on {}...", addr);
    let listener = TcpListener::bind(addr).await?;
    println!(
        "HTTP server listening on http://{}, forwarding to QUIC",
        addr
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
                        let content = format!("找不到客户端: {}", client_id);
                        let response = format!(
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n\r\n{}\r\n",
                            content.len(),
                            content
                        );
                        tcp_writer
                            .write_all(response.as_bytes())
                            .await
                            .unwrap_or_else(|e| {
                                eprintln!("[ERROR] Failed to write to TCP client: {}", e)
                            });
                        tcp_writer.flush().await.unwrap_or_else(|e| {
                            eprintln!("[ERROR] Failed to flush TCP client: {}", e)
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
