use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
    time::Duration,
};

use moka::future::Cache;
use serde_json::Value;

use crate::transport::base::TransportConnection;

pub const DEFAULT_CLIENT_ID: &str = "default_client_id";

#[derive(Clone)]
pub struct TransportSession {
    pub conn: Arc<dyn TransportConnection + Send + Sync + 'static>,
    pub meta: HashMap<String, Value>,
}

pub static TRANSPORT_SESSION_CACHE: LazyLock<Cache<String, TransportSession>> =
    LazyLock::new(|| {
        Cache::builder()
            .time_to_live(Duration::from_secs(60))
            .build()
    });

pub async fn insert_session(id: String, session: TransportSession) {
    TRANSPORT_SESSION_CACHE.insert(id, session).await;
}

pub async fn remove_session(id: &str) {
    TRANSPORT_SESSION_CACHE.invalidate(id).await;
}

pub async fn get_session(id: &str) -> Option<TransportSession> {
    TRANSPORT_SESSION_CACHE.get(id).await
}

pub async fn get_default_session() -> Option<TransportSession> {
    get_session(DEFAULT_CLIENT_ID).await
}

pub async fn refresh_session_by_id(id: &str) {
    if let Some(session) = TRANSPORT_SESSION_CACHE.get(id).await {
        TRANSPORT_SESSION_CACHE
            .insert(id.to_string(), session)
            .await;
    }
}
