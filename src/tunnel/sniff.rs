use anyhow::Result;
use httparse::Request;
use rustls::server::Acceptor;
use std::collections::HashMap;
use std::io::Cursor;
use tokio::net::tcp::OwnedReadHalf;

#[derive(Debug)]
pub struct SniffResult {
    pub tunnel_id: String,
    pub host: String,
    pub is_https: bool,
}

pub async fn sniff_tcp(tcp_stream: &mut OwnedReadHalf) -> Result<SniffResult> {
    let mut peek_buffer = [0u8; 4096];
    let n = tcp_stream.peek(&mut peek_buffer).await?;
    if n == 0 {
        return Err(anyhow::anyhow!("No data available"));
    }
    let data = &peek_buffer[..n];
    if let Some(result) = sniff_http(data) {
        return Ok(result);
    }
    if let Some(result) = sniff_tls_sni_safe(&peek_buffer[..n])? {
        return Ok(result);
    }

    Err(anyhow::anyhow!("Cannot sniff host"))
}
fn sniff_http(buf: &[u8]) -> Option<SniffResult> {
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut request = Request::new(&mut headers);

    if let Ok(httparse::Status::Complete(_)) = request.parse(buf) {
        let headers_map: HashMap<_, _> = request
            .headers
            .iter()
            .map(|h| {
                (
                    h.name.to_string(),
                    String::from_utf8_lossy(h.value).to_string(),
                )
            })
            .collect();

        let host_header = headers_map
            .get("Host")
            .unwrap_or(&"my-secret-token.localhost".to_string())
            .to_string();

        let host = if host_header.contains(':') {
            host_header.clone()
        } else {
            format!("{}:80", host_header)
        };

        let host_without_port = host_header.split(':').next().unwrap_or(&host_header);

        return Some(SniffResult {
            tunnel_id: host_without_port
                .split('.')
                .next()
                .unwrap_or("my-secret-token")
                .to_string(),
            host,
            is_https: false,
        });
    }
    None
}

fn sniff_tls_sni_safe(buf: &[u8]) -> Result<Option<SniffResult>> {
    let mut acceptor = Acceptor::default();
    let mut cursor = Cursor::new(buf);

    acceptor.read_tls(&mut cursor)?;
    match acceptor.accept() {
        Ok(Some(accepted)) => {
            if let Some(sni) = accepted.client_hello().server_name() {
                let host_without_port = sni.to_string();
                let host = format!("{}:443", host_without_port);
                return Ok(Some(SniffResult {
                    tunnel_id: host_without_port
                        .split('.')
                        .next()
                        .unwrap_or("my-secret-token")
                        .to_string(),
                    host,
                    is_https: true,
                }));
            }
            Ok(None)
        }
        Ok(None) => Ok(None),
        Err((e, _alert)) => Err(anyhow::anyhow!("TLS parse error: {:?}", e)),
    }
}
