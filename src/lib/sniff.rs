use httparse::Request;
use std::collections::HashMap;
/// HTTP 请求信息结构
#[derive(Debug)]
pub struct HttpRequestInfo {
    pub method: String,
    pub url: String,
    pub http_version: String,
    pub headers: HashMap<String, String>,
    pub body_size: usize,
}

pub async fn sniff_tcp(
    tcp_stream: &mut tokio::net::tcp::OwnedReadHalf,
) -> anyhow::Result<HttpRequestInfo> {
    let mut peek_buffer = [0; 2048];
    let n = tcp_stream.peek(&mut peek_buffer).await.map_or(0, |n| n);
    if n == 0 {
        return Err(anyhow::anyhow!("No data available"));
    }
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut request = Request::new(&mut headers);
    let parsed = request.parse(&peek_buffer[..n]);
    match parsed {
        Ok(httparse::Status::Complete(_)) => {
            let headers = request
                .headers
                .iter()
                .map(|h| {
                    (
                        h.name.to_string(),
                        String::from_utf8_lossy(h.value).to_string(),
                    )
                })
                .collect::<HashMap<String, String>>();
            let request_info = HttpRequestInfo {
                method: request.method.unwrap_or("GET").to_string(),
                url: request.path.unwrap_or("/").to_string(),
                http_version: request.version.unwrap_or(1).to_string(),
                headers: headers.clone(),
                body_size: headers
                    .get("Content-Length")
                    .unwrap_or(&String::from("0"))
                    .parse::<usize>()
                    .unwrap_or(0),
            };
            Ok(request_info)
        }
        Ok(httparse::Status::Partial) => Err(anyhow::anyhow!("Failed to parse request1")),
        Err(e) => Err(anyhow::anyhow!("Failed to parse request: {}", e)),
    }
}
