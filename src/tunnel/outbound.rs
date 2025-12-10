use crate::transport::base::TransportStream;
use crate::tunnel::common::FORWARD_TO_KEY;
use crate::tunnel::packet::TunnelPacket;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tracing::{error, info};

pub async fn tcp_outbound(
    mut stream_reader: ReadHalf<Box<dyn TransportStream>>,
    mut stream_writer: WriteHalf<Box<dyn TransportStream>>,
    packet: TunnelPacket,
    default_forward_to: Option<String>,
) -> anyhow::Result<()> {
    let forward_target = match default_forward_to {
        Some(forward_to) => forward_to,
        None => match packet.meta.get(FORWARD_TO_KEY).and_then(|v| v.as_str()) {
            Some(forward_to) => forward_to.to_string(),
            None => "".to_string(),
        },
    };
    if forward_target.is_empty() {
        return Err(anyhow::anyhow!("Tcp outbound forward target is empty"));
    }
    info!("Forwarding to: {}", forward_target);
    let upstream = TcpStream::connect(forward_target).await?;
    let (mut upstream_reader, mut upstream_writer) = tokio::io::split(upstream);
    let send = tokio::spawn(async move {
        if let Err(e) = tokio::io::copy(&mut stream_reader, &mut upstream_writer).await {
            error!("copy stream -> upstream error: {:?}", e);
        }
    });
    let receive = tokio::spawn(async move {
        if let Err(e) = tokio::io::copy(&mut upstream_reader, &mut stream_writer).await {
            error!("copy upstream -> stream error: {:?}", e);
        }
    });
    let res = tokio::try_join!(send, receive);
    if let Err(e) = res {
        error!("copy stream -> upstream error: {:?}", e);
    }
    Ok(())
}
