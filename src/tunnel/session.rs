use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use dashmap::DashMap;
use serde_json::Value;

use crate::transport::base::TransportConnection;

pub const DEFAULT_CLIENT_ID: &str = "default_client_id";

#[derive(Clone)]
pub struct TransportSession {
    pub conn: Arc<dyn TransportConnection + Send + Sync + 'static>,
    pub meta: HashMap<String, Value>,
    pub ping_at: tokio::time::Instant,
}

pub static TRANSPORT_SESSION_MAP: LazyLock<DashMap<String, TransportSession>> =
    LazyLock::new(|| DashMap::new());

pub fn get_session(id: &str) -> Option<TransportSession> {
    TRANSPORT_SESSION_MAP
        .get(id)
        .map(|session| session.value().clone())
}

pub fn get_default_session() -> Option<TransportSession> {
    get_session(DEFAULT_CLIENT_ID)
}

pub async fn clear_expired_sessions() {
    TRANSPORT_SESSION_MAP
        .iter()
        .filter(|session| session.value().ping_at.elapsed().as_secs() > 60)
        .for_each(|session| {
            TRANSPORT_SESSION_MAP.remove(session.key());
        });
}
