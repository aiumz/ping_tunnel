use anyhow::Result;
use std::net::ToSocketAddrs;

// constants
pub const FORWARD_TO_KEY: &str = "X-Tunnel-Forward-To";
pub const AUTH_TOKEN_KEY: &str = "X-Tunnel-Token";
pub const DEVICE_NAME_KEY: &str = "device_name";
pub const HEADER_FIXED_LEN: usize = 5;
pub const MAX_DATA_LEN: usize = 1024;
pub const MAX_SNIFF_LEN: usize = 2048;

// utils
pub fn get_client_id_from_token(token: &str) -> String {
    token.to_string()
}

pub fn resolve_socket_addr(addr: &str) -> Result<String> {
    let (host, port_str) = addr
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("Invalid address: {}", addr))?;

    let port: u16 = port_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid port in address: {}", addr))?;

    let mut addrs_iter = (host, port)
        .to_socket_addrs()
        .map_err(|_| anyhow::anyhow!("Cannot resolve address: {}", addr))?;

    let socket_addr = addrs_iter
        .next()
        .ok_or_else(|| anyhow::anyhow!("No valid IP found for address: {}", addr))?;

    Ok(socket_addr.to_string())
}
