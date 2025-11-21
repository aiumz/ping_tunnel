use std::{
    sync::{Arc, LazyLock},
    time::Instant,
};

use dashmap::DashMap;

use crate::lib::packet::TunnelMeta;

#[derive(Debug, Clone)]
pub struct ConnectionSession {
    pub conn: quinn::Connection,
    pub ping_at: Instant,
    pub meta: TunnelMeta,
}

pub type ConnectionSessionMap = Arc<DashMap<String, ConnectionSession>>;

pub static CONNECTIONS: LazyLock<ConnectionSessionMap> = LazyLock::new(|| Arc::new(DashMap::new()));
