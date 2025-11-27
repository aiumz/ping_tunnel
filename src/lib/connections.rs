use std::{
    sync::{Arc, LazyLock},
    time::Instant,
};

use crate::lib::packet::TunnelMeta;
use dashmap::DashMap;
use serde_json::Value;

pub const DEFAULT_CLIENT_ID: &str = "default_client_id";

#[derive(Debug, Clone)]
pub struct ConnectionSession {
    pub conn: quinn::Connection,
    pub ping_at: Instant,
    pub meta: TunnelMeta,
}
#[allow(dead_code)]
impl ConnectionSession {
    pub fn new(conn: quinn::Connection, meta: TunnelMeta) -> Self {
        Self {
            conn,
            ping_at: Instant::now(),
            meta,
        }
    }
    pub fn set_meta(&mut self, key: &str, value: &Value) {
        self.meta.insert(key.to_string(), value.clone());
    }
    pub fn get_meta(&self, key: &str) -> Option<Value> {
        self.meta.get(key).map(|v| v.clone())
    }
}

pub type ConnectionSessionMap = Arc<DashMap<String, ConnectionSession>>;

pub static CONNECTIONS: LazyLock<ConnectionSessionMap> = LazyLock::new(|| Arc::new(DashMap::new()));

pub async fn get_default_conn() -> anyhow::Result<quinn::Connection> {
    let mut max_retry = 10;
    while max_retry > 0 {
        if let Some(session) = CONNECTIONS.get(DEFAULT_CLIENT_ID) {
            return Ok(session.value().conn.clone());
        }
        max_retry -= 1;
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
    Err(anyhow::anyhow!("Default connection not found"))
}
