#[cfg(feature = "ssr")]
pub(crate) fn http_client() -> &'static reqwest::Client {
    static C: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    C.get_or_init(build_http_client)
}

#[cfg(feature = "ssr")]
fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to build timed notifier HTTP client");
            reqwest::Client::new()
        })
}

#[cfg(feature = "ssr")]
mod bark;
#[cfg(feature = "ssr")]
mod bus;
#[cfg(feature = "ssr")]
mod channels;
#[cfg(feature = "ssr")]
mod discord;
#[cfg(feature = "ssr")]
mod inapp;
#[cfg(feature = "ssr")]
mod smtp;
#[cfg(feature = "ssr")]
mod telegram;

#[cfg(feature = "ssr")]
pub use bus::{build_notifier, Notifier, NotifyBus};
#[cfg(feature = "ssr")]
pub use channels::{
    create_channel, delete_channel, list_channels, test_channel, NotifyChannelSummary,
    MAX_CHANNEL_NAME_CHARS,
};

#[cfg(all(test, feature = "ssr"))]
mod tests {
    #[test]
    fn notifier_http_client_builder_succeeds() {
        let _ = super::build_http_client();
    }
}

/// Throwaway loopback HTTP server used by the provider `send()` tests. It
/// records the first request's method + path + raw body, replies with a
/// caller-chosen status line + body, then stops. This exercises the real
/// reqwest request-building path inside `Notifier::send` WITHOUT touching the
/// network beyond 127.0.0.1, so we can assert the request shape and — on an
/// error response — that the recorded `notify_delivery.error` never echoes
/// any secret material (device key, token, webhook).
#[cfg(all(test, feature = "ssr"))]
pub(crate) mod test_server {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    /// What the loopback server captured from the single request it served.
    #[derive(Debug, Clone)]
    pub(crate) struct CapturedRequest {
        pub method: String,
        pub path: String,
        pub body: String,
    }

    /// A running loopback recorder. `base_url` is the `http://127.0.0.1:PORT`
    /// origin to point a notifier at; awaiting `captured` yields the one
    /// request the server received and then the server shuts down.
    pub(crate) struct RecordingServer {
        pub base_url: String,
        captured: oneshot::Receiver<CapturedRequest>,
    }

    impl RecordingServer {
        /// Bind 127.0.0.1:0 and serve exactly one request, answering with
        /// `status_line` (e.g. "200 OK" / "401 Unauthorized") and `resp_body`.
        pub(crate) async fn start(status_line: &'static str, resp_body: &'static str) -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0))
                .await
                .expect("bind loopback listener");
            let addr = listener.local_addr().expect("local addr");
            let base_url = format!("http://127.0.0.1:{}", addr.port());
            let (tx, rx) = oneshot::channel();

            tokio::spawn(async move {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };

                // Read the full request: headers, then exactly Content-Length
                // bytes of body (reqwest always sends Content-Length for a
                // JSON body, so no chunked decoding is needed).
                let mut buf: Vec<u8> = Vec::with_capacity(1024);
                let mut tmp = [0u8; 1024];
                let header_end = loop {
                    let n = match socket.read(&mut tmp).await {
                        Ok(0) | Err(_) => break None,
                        Ok(n) => n,
                    };
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
                        break Some(pos);
                    }
                };
                let Some(header_end) = header_end else {
                    return;
                };

                let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
                let mut lines = head.split("\r\n");
                let request_line = lines.next().unwrap_or_default();
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or_default().to_string();
                let path = parts.next().unwrap_or_default().to_string();

                let content_length = lines
                    .filter_map(|l| l.split_once(':'))
                    .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);

                let body_start = header_end + 4; // skip the CRLFCRLF
                let mut body = buf[body_start..].to_vec();
                while body.len() < content_length {
                    let n = match socket.read(&mut tmp).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    body.extend_from_slice(&tmp[..n]);
                }
                let body =
                    String::from_utf8_lossy(&body[..content_length.min(body.len())]).into_owned();

                let response = format!(
                    "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp_body}",
                    resp_body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
                let _ = socket.shutdown().await;

                let _ = tx.send(CapturedRequest { method, path, body });
            });

            Self {
                base_url,
                captured: rx,
            }
        }

        /// Await the single recorded request. Panics if the server task died
        /// before serving a request.
        pub(crate) async fn captured(self) -> CapturedRequest {
            self.captured.await.expect("server recorded a request")
        }
    }

    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }
}
