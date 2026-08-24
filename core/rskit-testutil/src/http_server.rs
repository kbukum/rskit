//! In-process HTTP server fake for transport tests.
//!
//! [`FakeHttpServer`] binds an ephemeral loopback port, accepts a single
//! connection, captures the raw request, and replies with a programmed
//! [`FakeResponse`]. It lets HTTP-client, discovery, and adapter tests exercise
//! real request/response wire behavior without a network dependency or a
//! per-crate hand-rolled listener.

use std::net::SocketAddr;

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// A programmed HTTP response served by [`FakeHttpServer`].
#[derive(Debug, Clone)]
pub struct FakeResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl FakeResponse {
    /// Create a response with the given status code, no extra headers, and an empty body.
    #[must_use]
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Add a response header.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Set the response body.
    #[must_use]
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    fn encode(&self) -> Vec<u8> {
        let mut head = format!(
            "HTTP/1.1 {} OK\r\nContent-Length: {}\r\n",
            self.status,
            self.body.len()
        );
        for (name, value) in &self.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

/// The raw HTTP request captured by [`FakeHttpServer`].
#[derive(Debug, Clone)]
pub struct CapturedRequest {
    raw: String,
}

impl CapturedRequest {
    /// The full raw request text (request line, headers, and body).
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The request line, e.g. `GET /v1/health HTTP/1.1`.
    #[must_use]
    pub fn request_line(&self) -> &str {
        self.raw.lines().next().unwrap_or_default()
    }

    /// Whether the raw request contains `needle`.
    #[must_use]
    pub fn contains(&self, needle: &str) -> bool {
        self.raw.contains(needle)
    }

    /// The first value of `name`, matched case-insensitively.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<String> {
        self.raw.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    }
}

/// An in-process HTTP server that serves one request from a programmed response.
///
/// The server spawns a background task that accepts a single connection, reads
/// the full request (respecting `Content-Length`), records it, then writes the
/// programmed [`FakeResponse`]. Retrieve the captured request with
/// [`FakeHttpServer::captured_request`] after driving the client under test.
pub struct FakeHttpServer {
    addr: SocketAddr,
    request: oneshot::Receiver<CapturedRequest>,
}

impl FakeHttpServer {
    /// Bind a loopback server that answers exactly one request with `response`.
    ///
    /// # Errors
    /// Returns an error when the ephemeral loopback port cannot be bound.
    pub async fn serve_once(response: FakeResponse) -> AppResult<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to bind fake http server: {error}"),
            )
        })?;
        let addr = listener.local_addr().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read fake http server address: {error}"),
            )
        })?;
        let (tx, request) = oneshot::channel();
        let payload = response.encode();

        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 1024];
            let header_end = loop {
                let Ok(read) = socket.read(&mut buffer).await else {
                    return;
                };
                if read == 0 {
                    break bytes.len();
                }
                bytes.extend_from_slice(&buffer[..read]);
                if let Some(pos) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    break pos + 4;
                }
            };
            let headers_text = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
            let content_length = headers_text
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            while bytes.len().saturating_sub(header_end) < content_length {
                let Ok(read) = socket.read(&mut buffer).await else {
                    break;
                };
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..read]);
            }
            let captured = CapturedRequest {
                raw: String::from_utf8_lossy(&bytes).into_owned(),
            };
            let _ = tx.send(captured);
            let _ = socket.write_all(&payload).await;
            let _ = socket.shutdown().await;
        });

        Ok(Self { addr, request })
    }

    /// The server address in `host:port` form (e.g. `127.0.0.1:54321`).
    #[must_use]
    pub fn address(&self) -> String {
        self.addr.to_string()
    }

    /// The bound socket address.
    #[must_use]
    pub fn socket_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Await the request captured from the single served connection.
    ///
    /// # Errors
    /// Returns an error when the server task ends before capturing a request
    /// (for example, when the connection closed before a full request arrived).
    pub async fn captured_request(self) -> AppResult<CapturedRequest> {
        self.request.await.map_err(|_| {
            AppError::new(
                ErrorCode::Internal,
                "fake http server captured no request before closing",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn send_raw(addr: &str, request: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    #[tokio::test]
    async fn serves_programmed_status_headers_and_body() {
        let server = FakeHttpServer::serve_once(
            FakeResponse::new(201)
                .with_header("X-Test", "yes")
                .with_body("hello"),
        )
        .await
        .unwrap();
        let addr = server.address();

        let response = send_raw(&addr, "GET /ping HTTP/1.1\r\nHost: x\r\n\r\n").await;

        assert!(response.starts_with("HTTP/1.1 201"));
        assert!(response.contains("X-Test: yes"));
        assert!(response.contains("Content-Length: 5"));
        assert!(response.ends_with("hello"));
    }

    #[tokio::test]
    async fn captures_request_line_headers_and_body() {
        let server = FakeHttpServer::serve_once(FakeResponse::new(200))
            .await
            .unwrap();
        let addr = server.address();

        let body = r#"{"name":"api"}"#;
        let request = format!(
            "PUT /register HTTP/1.1\r\nHost: x\r\nX-Token: secret\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let _ = send_raw(&addr, &request).await;

        let captured = server.captured_request().await.unwrap();
        assert_eq!(captured.request_line(), "PUT /register HTTP/1.1");
        assert_eq!(captured.header("x-token").as_deref(), Some("secret"));
        assert!(captured.contains(r#""name":"api""#));
    }

    #[tokio::test]
    async fn captures_partial_request_when_client_closes_before_terminator() {
        let server = FakeHttpServer::serve_once(FakeResponse::new(200))
            .await
            .unwrap();
        let addr = server.address();

        // Send a request line but close before the header terminator.
        let mut stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
        stream
            .write_all(b"GET /partial HTTP/1.1\r\n")
            .await
            .unwrap();
        stream.shutdown().await.unwrap();

        let captured = server.captured_request().await.unwrap();
        assert_eq!(captured.request_line(), "GET /partial HTTP/1.1");
    }

    #[tokio::test]
    async fn address_matches_socket_addr() {
        let server = FakeHttpServer::serve_once(FakeResponse::new(200))
            .await
            .unwrap();
        assert_eq!(server.address(), server.socket_addr().to_string());
    }
}
