use ping_tunnel::tunnel::supernode::start_server;
use std::env;

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
    start_server(quic_bind_addr, tcp_bind_addr, cert_path, key_path).await
}
