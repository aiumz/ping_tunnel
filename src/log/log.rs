use chrono::Local;
use std::fmt as StdFmt;
use std::sync::Once;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;

struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> StdFmt::Result {
        let now = Local::now();
        write!(w, "{}", now.format("%Y-%m-%d %H:%M:%S.%3f"))
    }
}

static INIT_LOG: Once = Once::new();

pub fn init_log() {
    INIT_LOG.call_once(|| {
        tracing_subscriber::fmt()
            .with_timer(LocalTimer)
            .with_env_filter(EnvFilter::from_default_env())
            .with_target(false)
            .init();
    });
}
