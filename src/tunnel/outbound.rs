use crate::tunnel::common::FORWARD_TO_KEY;
use crate::tunnel::packet::TunnelCommandPacket;
use crate::transport::base::TransportStream;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;

pub async fn forward_to_tcp(
    mut stream_reader: ReadHalf<Box<dyn TransportStream>>,
    mut stream_writer: WriteHalf<Box<dyn TransportStream>>,
    packet: TunnelCommandPacket,
    default_forward_to: Option<String>,
) -> anyhow::Result<()> {
    let forward_target = match packet.meta.get(FORWARD_TO_KEY).and_then(|v| v.as_str()) {
        Some(forward_to) => forward_to.to_string(),
        None => {
            if let Some(forward_to) = default_forward_to {
                forward_to
            } else {
                "".to_string()
            }
        }
    };
    println!("[QUIC Client] Forwarding to: {}", forward_target);
    let upstream = TcpStream::connect(forward_target).await?;
    let (mut upstream_reader, mut upstream_writer) = tokio::io::split(upstream);
    tokio::spawn(async move {
        if let Err(e) = tokio::io::copy(&mut stream_reader, &mut upstream_writer).await {
            eprintln!("[QUIC Client] copy stream -> upstream error: {:?}", e);
        }
    });
    tokio::spawn(async move {
        if let Err(e) = tokio::io::copy(&mut upstream_reader, &mut stream_writer).await {
            eprintln!("[QUIC Client] copy upstream -> stream error: {:?}", e);
        }
    });
    Ok(())
}
