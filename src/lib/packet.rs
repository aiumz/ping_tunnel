use crate::lib::common::{HEADER_FIXED_LEN, MAX_DATA_LEN};
use serde_json;
use serde_json::Value;
use std::collections::HashMap;
/// 隧道命令
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
pub struct TunnelCommandPacket {
    pub command: TunnelCommand,
    pub length: u32,
    pub meta: TunnelMeta,
}

impl TunnelCommandPacket {
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

    pub async fn read_command(stream: &mut quinn::RecvStream) -> Result<Self, anyhow::Error> {
        let mut buffer = [0u8; HEADER_FIXED_LEN];
        let result = stream.read_exact(&mut buffer);
        let (command, length) = match result.await {
            Ok(()) => (
                buffer[0],
                u32::from_be_bytes(buffer[1..HEADER_FIXED_LEN].try_into().unwrap()),
            ),
            Err(e) => {
                eprintln!("[ERROR] Error reading stream: {:?}", e);
                return Err(anyhow::anyhow!("Error reading stream"));
            }
        };
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
        let result = stream.read_exact(&mut data_buffer);
        let data = match result.await {
            Ok(()) => Self::decode_meta(&data_buffer),
            Err(e) => {
                eprintln!("[ERROR] Error reading stream: {:?}", e);
                return Err(anyhow::anyhow!("Error reading stream"));
            }
        };

        let result = Self {
            command,
            length,
            meta: data,
        };
        // println!("[DEBUG]  {:?}", result);
        Ok(result)
    }
}
