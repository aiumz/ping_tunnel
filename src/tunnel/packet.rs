use crate::{
    transport::base::TransportStream,
    tunnel::{
        common::{HEADER_FIXED_LEN, MAX_DATA_LEN},
        session::get_default_session,
    },
};
use serde_json;
use serde_json::Value;
use std::{collections::HashMap, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, WriteHalf};
use tracing::debug;

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum TunnelCommand {
    Ping = 0,
    Pong = 1,
    Auth = 2,
    AuthResult = 3,
    Forward = 4,
    SetSessionMeta = 5,
}

pub type TunnelMeta = HashMap<String, Value>;

#[derive(Debug, Clone)]
pub struct TunnelPacket {
    pub command: TunnelCommand,
    pub length: u32,
    pub meta: TunnelMeta,
}

impl TunnelPacket {
    pub fn new(command: TunnelCommand, meta: &TunnelMeta) -> Self {
        Self {
            command: command,
            length: Self::encode_meta(&meta).len() as u32,
            meta: meta.clone(),
        }
    }

    pub fn encode_meta(meta: &TunnelMeta) -> Vec<u8> {
        serde_json::to_vec(meta).unwrap()
    }

    pub fn decode_meta(bytes: &[u8]) -> TunnelMeta {
        serde_json::from_slice(bytes).unwrap()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.length as usize + HEADER_FIXED_LEN);
        buf.extend_from_slice(&(self.command as u8).to_be_bytes());
        buf.extend_from_slice(&self.length.to_be_bytes());
        buf.extend_from_slice(&Self::encode_meta(&self.meta));
        buf
    }

    pub async fn read_command(
        reader: &mut (dyn AsyncRead + Unpin + Send),
    ) -> Result<Self, anyhow::Error> {
        let mut buffer = [0u8; HEADER_FIXED_LEN];
        reader.read_exact(&mut buffer).await?;
        let (command, length) = (
            buffer[0],
            u32::from_be_bytes(buffer[1..HEADER_FIXED_LEN].try_into().unwrap()),
        );
        let command = match command as u8 {
            0 => TunnelCommand::Ping,
            1 => TunnelCommand::Pong,
            2 => TunnelCommand::Auth,
            3 => TunnelCommand::AuthResult,
            4 => TunnelCommand::Forward,
            _ => return Err(anyhow::anyhow!("Invalid command type")),
        };

        if length > MAX_DATA_LEN as u32 {
            return Err(anyhow::anyhow!("Data length is too large {}", length));
        }

        let mut data_buffer = vec![0u8; length as usize];
        reader.read_exact(&mut data_buffer).await?;

        let result = Self {
            command,
            length,
            meta: Self::decode_meta(&data_buffer),
        };
        debug!("Received packet: {:?}", result);
        Ok(result)
    }
}

pub async fn send_packet(
    command: TunnelCommand,
    meta: &TunnelMeta,
) -> Result<TunnelPacket, anyhow::Error> {
    if let Some(session) = get_default_session().await {
        tokio::time::timeout(Duration::from_secs(10), async {
            let stream = session
                .conn
                .open_stream()
                .await
                .map_err(|e| anyhow::anyhow!("Connection closed: {}", e))?;
            let (mut recv_stream, mut send_stream) = tokio::io::split(stream);
            let command_packet = TunnelPacket::new(command, meta);
            send_stream
                .write_all(&command_packet.to_bytes())
                .await
                .map_err(|e| anyhow::anyhow!("Write error: {}", e))?;
            send_stream
                .flush()
                .await
                .map_err(|e| anyhow::anyhow!("Flush error: {}", e))?;
            let response_packet = TunnelPacket::read_command(&mut recv_stream)
                .await
                .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;
            let _ = send_stream.shutdown().await;
            Ok(response_packet)
        })
        .await
        .map_err(|e| {
            let err_msg = e.to_string();
            if err_msg.contains("deadline has elapsed") || err_msg.contains("timeout") {
                anyhow::anyhow!("Command timeout")
            } else {
                anyhow::anyhow!("Command error: {}", e)
            }
        })?
    } else {
        Err(anyhow::anyhow!("Default connection not found"))
    }
}

pub async fn response_packet(
    mut stream: WriteHalf<Box<dyn TransportStream>>,
    command: TunnelCommand,
    meta: &TunnelMeta,
) -> Result<TunnelPacket, anyhow::Error> {
    let command_packet = TunnelPacket::new(command, meta);
    let command_bytes = command_packet.to_bytes();
    if let Err(e) = stream.write_all(&command_bytes).await {
        return Err(anyhow::anyhow!(e));
    }
    if let Err(e) = stream.flush().await {
        return Err(anyhow::anyhow!(e));
    }
    if let Err(e) = stream.shutdown().await {
        return Err(anyhow::anyhow!(e));
    }
    debug!("Responded packet: {:?}", command_packet);
    Ok(command_packet)
}
