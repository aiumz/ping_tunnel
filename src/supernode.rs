use ping_tunnel::tunnel::supernode::start_server;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    let mut quic_bind_addr = "0.0.0.0:4433".to_string();
    let mut tcp_bind_addr = "0.0.0.0:4432".to_string();
    let mut cert_path = "./cert/cert.pem".to_string();
    let mut key_path = "./cert/key.pem".to_string();
    if args.len() == 5 {
        quic_bind_addr = args[1].clone();
        tcp_bind_addr = args[2].clone();
        cert_path = args[3].clone();
        key_path = args[4].clone();
    }
    start_server(quic_bind_addr, tcp_bind_addr, cert_path, key_path).await
}
