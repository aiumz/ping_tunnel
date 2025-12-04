use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::OnceCell;

use crate::transport::base::{TransportConnection, TransportStream};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub type OnAcceptStream = Arc<
    dyn Fn(
            Arc<dyn TransportConnection + Send + Sync>,
            Box<dyn TransportStream>,
        ) -> BoxFuture<'static, Result<()>>
        + Send
        + Sync,
>;

pub static ON_ACCEPT_STREAM: OnceCell<OnAcceptStream> = OnceCell::const_new();

pub fn register_on_accept_stream<F, Fut>(cb: F)
where
    F: Fn(Arc<dyn TransportConnection + Send + Sync>, Box<dyn TransportStream>) -> Fut
        + Send
        + Sync
        + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    let boxed: OnAcceptStream = Arc::new(move |conn, stream| Box::pin(cb(conn, stream)));

    ON_ACCEPT_STREAM.set(boxed).ok();
}

pub async fn call_on_accept_stream(
    conn: Arc<dyn TransportConnection + Send + Sync>,
    stream: Box<dyn TransportStream>,
) -> Result<()> {
    let cb = ON_ACCEPT_STREAM.get().expect("not set");
    cb(conn, stream).await
}
