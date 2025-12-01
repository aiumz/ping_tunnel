use ping_tunnel::tunnel::edge::connect_to_server;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 4 {
        eprintln!("Usage: {} <server_addr:port> <token> <forward_to>", args[0]);
        std::process::exit(1);
    }

    let server_addr = args[1].clone();
    let token = args[2].clone();
    let forward_to = args[3].clone();
    connect_to_server(server_addr, token, forward_to).await
}
