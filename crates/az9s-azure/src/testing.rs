//! Test helpers: a static token source and a tiny local HTTP server that
//! stands in for Azure. Enabled for this crate's tests and, for other crates,
//! through the `test-util` feature.

use crate::resource_graph::TokenSource;
use crate::Result;
use futures::future::BoxFuture;
use futures::FutureExt;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Always returns the same token.
pub struct StaticToken(pub String);

impl StaticToken {
    /// A JWT-shaped token whose payload carries `tid` (tenant ID).
    pub fn for_tenant(tenant_id: &str) -> Self {
        let payload = base64::encode_config(
            format!(r#"{{"tid":"{tenant_id}"}}"#),
            base64::URL_SAFE_NO_PAD,
        );
        Self(format!("header.{payload}.signature"))
    }
}

impl TokenSource for StaticToken {
    fn token(&self) -> BoxFuture<'_, Result<String>> {
        let t = self.0.clone();
        async move { Ok(t) }.boxed()
    }
}

/// A canned HTTP response.
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, &'static str)>,
    pub body: String,
    /// Simulated server latency before the response is sent.
    pub delay: std::time::Duration,
}

impl Reply {
    pub fn after(mut self, delay: std::time::Duration) -> Self {
        self.delay = delay;
        self
    }
}

/// 200 with a JSON body.
pub fn ok(body: Value) -> Reply {
    Reply {
        status: 200,
        headers: vec![],
        body: body.to_string(),
        delay: Default::default(),
    }
}

/// An error status with an ARM-style error body.
pub fn error(status: u16, message: &str) -> Reply {
    Reply {
        status,
        headers: vec![],
        body: serde_json::json!({"error": {"code": "Test", "message": message}}).to_string(),
        delay: Default::default(),
    }
}

/// Request bodies the server has received, in order.
pub type Requests = Arc<Mutex<Vec<Value>>>;

/// Serve `replies` in order, one per request. Returns the base URL.
pub async fn serve_sequence(replies: Vec<Reply>) -> (String, Requests) {
    let replies = Mutex::new(replies.into_iter());
    serve(move |_| {
        replies
            .lock()
            .unwrap()
            .next()
            .unwrap_or_else(|| error(500, "no more canned replies"))
    })
    .await
}

/// Serve every request with `respond(request_body)`. Returns the base URL.
pub async fn serve<F>(respond: F) -> (String, Requests)
where
    F: Fn(&Value) -> Reply + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests: Requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let respond = Arc::new(respond);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let seen = seen.clone();
            let respond = respond.clone();
            tokio::spawn(async move {
                let Some(body) = read_request_body(&mut sock).await else {
                    return;
                };
                seen.lock().unwrap().push(body.clone());
                let reply = respond(&body);
                if !reply.delay.is_zero() {
                    tokio::time::sleep(reply.delay).await;
                }
                let mut response = format!(
                    "HTTP/1.1 {} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
                    reply.status,
                    reply.body.len()
                );
                for (k, v) in &reply.headers {
                    response.push_str(&format!("{k}: {v}\r\n"));
                }
                response.push_str("\r\n");
                response.push_str(&reply.body);
                let _ = sock.write_all(response.as_bytes()).await;
            });
        }
    });
    (format!("http://{addr}"), requests)
}

/// Read one HTTP request and return its JSON body (`Null` if empty).
async fn read_request_body(sock: &mut tokio::net::TcpStream) -> Option<Value> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let (head_end, content_length) = loop {
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            break (pos + 4, len);
        }
    };
    while buf.len() < head_end + content_length {
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Some(serde_json::from_slice(&buf[head_end..head_end + content_length]).unwrap_or(Value::Null))
}
