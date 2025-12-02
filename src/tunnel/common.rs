use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{Duration, timeout};
use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    sync::RwLock,
};

pub const FORWARD_TO_KEY: &str = "X-Tunnel-Forward-To";
pub const AUTH_TOKEN_KEY: &str = "X-Tunnel-Token";
pub const DEVICE_NAME_KEY: &str = "device_name";
pub const HEADER_FIXED_LEN: usize = 5;
pub const MAX_DATA_LEN: usize = 1024;
pub const MAX_SNIFF_LEN: usize = 2048;

pub fn get_client_id_from_token(token: &str) -> String {
    token.to_string()
}

const IDLE_TIMEOUT: Option<Duration> = Some(Duration::from_secs(120));
pub async fn copy_buffer<R, W>(mut r: R, mut w: W) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = if let Some(t) = IDLE_TIMEOUT {
            timeout(t, r.read(&mut buf)).await??
        } else {
            r.read(&mut buf).await?
        };

        if n == 0 {
            break;
        }
        w.write_all(&buf[..n]).await?;
    }
    Ok(())
}
