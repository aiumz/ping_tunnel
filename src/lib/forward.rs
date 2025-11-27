use std::collections::HashMap;
use std::time::Duration;

use crate::lib::common::FORWARD_TO_KEY;
use crate::lib::packet::{TunnelCommand, TunnelCommandPacket, TunnelMeta};
use bytes::Bytes;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
const BUF_SZ: usize = 64 * 1024;
pub async fn quic_to_tcp(
    forward_to: &str,
    mut recv: quinn::RecvStream,
    mut send: quinn::SendStream,
) {
    let tcp_stream = match TcpStream::connect(forward_to).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "[FORWARD] Failed to connect TCP server {}: {}",
                forward_to, e
            );
            let err_msg = format!(
                "HTTP/1.1 502 Bad Gateway\r\nContent-Length: {}\r\n\r\nFailed to connect: {}",
                e.to_string().len() + 30,
                e
            );
            let _ = send.write_chunk(Bytes::from(err_msg)).await;
            let _ = send.finish();
            return;
        }
    };

    let (mut tcp_reader, mut tcp_writer) = tcp_stream.into_split();
    // 接收 QUIC 数据并写入 TCP 客户端
    tokio::spawn(async move {
        loop {
            match recv.read_chunk(BUF_SZ, false).await {
                Ok(Some(chunk)) => {
                    if let Err(e) = tcp_writer.write_all(&chunk.bytes).await {
                        eprintln!("[FORWARD] TCP write error: {}", e);
                        break;
                    }
                }
                Ok(None) => {
                    if let Err(e) = tcp_writer.flush().await {
                        eprintln!("[FORWARD] TCP flush error: {}", e);
                        break;
                    }
                    break;
                }
                Err(e) => {
                    eprintln!("[FORWARD] QUIC->TCP read error: {}", e);
                    break;
                }
            }
        }
        if let Err(e) = tcp_writer.shutdown().await {
            eprintln!("[FORWARD] Failed to shutdown TCP writer: {}", e);
        }
    });
    // 发送 TCP 数据到 QUIC
    tokio::spawn(async move {
        let mut buf = [0u8; BUF_SZ];
        loop {
            match tcp_reader.read(&mut buf).await {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    if let Err(e) = send.write_chunk(Bytes::copy_from_slice(&buf[..n])).await {
                        eprintln!("[FORWARD] QUIC write error: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("[FORWARD] TCP read error: {}", e);
                    break;
                }
            }
        }

        if let Err(e) = send.finish() {
            eprintln!("[FORWARD] Failed to finish QUIC send stream: {}", e);
        }
    });
}

pub async fn tcp_to_quic(
    mut tcp_reader: tokio::net::tcp::OwnedReadHalf,
    mut tcp_writer: tokio::net::tcp::OwnedWriteHalf,
    conn: quinn::Connection,
    forward_to: String,
) {
    let (mut send, mut recv) = match conn.open_bi().await {
        Ok(streams) => streams,
        Err(e) => {
            eprintln!("[ERROR] Failed to open bidirectional stream: {}", e);
            return;
        }
    };
    // 发送 TCP 数据到 QUIC
    tokio::spawn(async move {
        let meta = HashMap::from([(FORWARD_TO_KEY.to_string(), Value::String(forward_to))]);
        let header = TunnelCommandPacket::new(TunnelCommand::Forward, &meta).to_bytes();
        if let Err(e) = send.write_chunk(Bytes::from(header)).await {
            eprintln!("[ERROR] Failed to send forward header: {}", e);
            return;
        }
        let mut buf = [0u8; BUF_SZ];
        loop {
            match tcp_reader.read(&mut buf).await {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    if let Err(e) = send.write_chunk(Bytes::copy_from_slice(&buf[..n])).await {
                        eprintln!("[ERROR] Failed to write chunk to QUIC: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("[ERROR] TCP read error: {}", e);
                    break;
                }
            }
        }

        // 完成 QUIC 发送流
        if let Err(e) = send.finish() {
            eprintln!("[ERROR] Failed to finish QUIC send stream: {}", e);
        }
    });

    // 接收 QUIC 数据并写入 TCP 客户端
    tokio::spawn(async move {
        loop {
            match recv.read_chunk(BUF_SZ, false).await {
                Ok(Some(chunk)) => {
                    if let Err(e) = tcp_writer.write_all(&chunk.bytes).await {
                        eprintln!("[ERROR] Failed to write to HTTP client: {}", e);
                        break;
                    }
                }
                Ok(None) => {
                    if let Err(e) = tcp_writer.flush().await {
                        eprintln!("[ERROR] Failed to flush TCP writer: {}", e);
                        break;
                    }
                    break;
                }
                Err(e) => {
                    eprintln!("[ERROR] QUIC recv error: {}", e);
                    break;
                }
            }
        }

        if let Err(e) = tcp_writer.shutdown().await {
            eprintln!("[ERROR] Failed to shutdown TCP writer: {}", e);
        }
    });
}

pub async fn command_to_quic(
    conn: quinn::Connection,
    command: TunnelCommand,
    meta: &TunnelMeta,
) -> Result<TunnelCommandPacket, anyhow::Error> {
    match tokio::time::timeout(Duration::from_secs(5), async {
        let (mut send, mut recv) = match conn.open_bi().await {
            Ok(streams) => streams,
            Err(e) => {
                eprintln!("[ERROR] Failed to open bidirectional stream: {}", e);
                return Err(anyhow::anyhow!(e.to_string()));
            }
        };
        let packet = TunnelCommandPacket::new(command, meta).to_bytes();
        if let Err(e) = send.write_chunk(Bytes::from(packet.clone())).await {
            eprintln!("[ERROR] Failed to write chunk to QUIC: {}", e);
            return Err(anyhow::anyhow!(e.to_string()));
        }
        if let Err(e) = send.finish() {
            eprintln!("[ERROR] Failed to finish QUIC send stream: {}", e);
            return Err(anyhow::anyhow!(e.to_string()));
        }
        let read_result = TunnelCommandPacket::read_command(&mut recv).await;
        match read_result {
            Ok(packet) => Ok(packet),
            Err(e) => {
                eprintln!("[ERROR] Failed to read tunnel header: {}", e);
                Err(anyhow::anyhow!(e))
            }
        }
    })
    .await
    {
        Ok(result) => result,
        Err(e) => {
            eprintln!("[ERROR] Command to QUIC timeout: {}", e);
            Err(anyhow::anyhow!(e))
        }
    }
}

pub async fn response_command(
    mut send: quinn::SendStream,
    response: TunnelCommandPacket,
) -> Result<(), anyhow::Error> {
    let packet = response.to_bytes();
    if let Err(e) = send.write_chunk(Bytes::from(packet)).await {
        return Err(anyhow::anyhow!(e.to_string()));
    }
    if let Err(e) = send.finish() {
        return Err(anyhow::anyhow!(e.to_string()));
    }
    Ok(())
}
