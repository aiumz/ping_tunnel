use ping_tunnel::tunnel::edge::start_client;
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
    start_client(server_addr, token, forward_to).await
}
