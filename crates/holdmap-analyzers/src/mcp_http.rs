//! Streamable HTTP for MCP analyzers: rmcp's client transport (sessions, protocol-version
//! header, JSON or SSE replies) on top of holdmap's existing `ureq` HTTPS client, so no
//! second HTTP and TLS stack is linked. Each message is one POST; replies are read whole,
//! capped and parsed. No redirects or environment proxies are followed, server text is never
//! kept, and the bearer token goes only to the approved URL. The optional standalone GET
//! stream isn't opened: analyzers only need replies to their own requests.

use crate::secrets::Secret;
use bytes::Bytes;
use futures::stream::{self, BoxStream, StreamExt};
use rmcp::model::{ClientJsonRpcMessage, ServerJsonRpcMessage};
use rmcp::transport::streamable_http_client::{
    AuthRequiredError, SseError, StreamableHttpClient, StreamableHttpClientTransportConfig,
    StreamableHttpError, StreamableHttpPostResponse,
};
use rmcp::transport::StreamableHttpClientTransport;
use sse_stream::{Sse, SseStream};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use ureq::http::{HeaderName, HeaderValue};

const SESSION: &str = "Mcp-Session-Id";

/// Why an HTTP exchange failed, in holdmap's words.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HttpError(&'static str);

/// The `ureq`-backed client.
#[derive(Clone)]
pub struct UreqClient {
    agent: ureq::Agent,
    cap: usize,
}

impl UreqClient {
    /// A client with a per-request time limit and a reply cap; redirects are refused.
    pub fn new(timeout: Duration, cap: usize) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            // Connect to the endpoint itself: an HTTP(S)_PROXY from the environment would
            // otherwise receive what holdmap sends to a local server.
            .proxy(None)
            .timeout_global(Some(timeout))
            .user_agent(concat!("holdmap/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self { agent, cap }
    }
}

/// rmcp's Streamable HTTP transport for `url`.
pub fn transport(
    url: &str,
    bearer: Option<&Secret>,
    cap: usize,
    timeout: Duration,
) -> StreamableHttpClientTransport<UreqClient> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(url.to_string());
    config.auth_header = bearer.map(|b| b.expose().to_string());
    config.reinit_on_expired_session = false;
    StreamableHttpClientTransport::with_client(UreqClient::new(timeout, cap), config)
}

struct Reply {
    status: u16,
    content_type: Option<String>,
    session: Option<String>,
    authenticate: Option<String>,
    body: Vec<u8>,
}

fn exchange(
    agent: &ureq::Agent,
    method: &'static str,
    uri: &str,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    cap: usize,
) -> Result<Reply, HttpError> {
    let mut req = ureq::http::Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let req = req
        .body(body.unwrap_or_default())
        .map_err(|_| HttpError("the request could not be built"))?;
    let mut resp = agent
        .run(req)
        .map_err(|_| HttpError("the server could not be reached"))?;
    let header = |name: &str| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let (content_type, session, authenticate) = (
        header("content-type"),
        header(SESSION),
        header("www-authenticate"),
    );
    let status = resp.status().as_u16();
    let body = if matches!(status, 202 | 204) || method == "DELETE" {
        Vec::new()
    } else {
        resp.body_mut()
            .with_config()
            .limit(cap as u64)
            .read_to_vec()
            .map_err(|_| HttpError("the reply was larger than the output limit or broken"))?
    };
    Ok(Reply {
        status,
        content_type,
        session,
        authenticate,
        body,
    })
}

fn headers(
    session: Option<&str>,
    auth: Option<&str>,
    custom: HashMap<HeaderName, HeaderValue>,
    accept: &str,
) -> Vec<(String, String)> {
    let mut out = vec![("accept".to_string(), accept.to_string())];
    if let Some(s) = session {
        out.push((SESSION.into(), s.into()));
    }
    if let Some(token) = auth {
        out.push(("authorization".into(), format!("Bearer {token}")));
    }
    for (k, v) in custom {
        if let Ok(v) = v.to_str() {
            out.push((k.as_str().to_string(), v.to_string()));
        }
    }
    out
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, HttpError> + Send + 'static,
) -> Result<T, StreamableHttpError<HttpError>> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| StreamableHttpError::Client(HttpError("the request was interrupted")))?
        .map_err(StreamableHttpError::Client)
}

fn sse(body: Vec<u8>) -> BoxStream<'static, Result<Sse, SseError>> {
    let chunks = stream::iter([Ok::<Bytes, std::io::Error>(Bytes::from(body))]);
    SseStream::from_bytes_stream(chunks).boxed()
}

impl StreamableHttpClient for UreqClient {
    type Error = HttpError;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let is_request = matches!(message, ClientJsonRpcMessage::Request(_));
        let body = serde_json::to_vec(&message).map_err(|_| {
            StreamableHttpError::Client(HttpError("the message could not be encoded"))
        })?;
        let mut hs = headers(
            session_id.as_deref(),
            auth_header.as_deref(),
            custom_headers,
            "application/json, text/event-stream",
        );
        hs.push(("content-type".into(), "application/json".into()));
        let (agent, cap) = (self.agent.clone(), self.cap);
        let reply = blocking(move || exchange(&agent, "POST", &uri, hs, Some(body), cap)).await?;
        match reply.status {
            202 | 204 => return Ok(StreamableHttpPostResponse::Accepted),
            401 => {
                return Err(StreamableHttpError::AuthRequired(AuthRequiredError::new(
                    reply.authenticate.unwrap_or_default(),
                )))
            }
            404 if session_id.is_some() => return Err(StreamableHttpError::SessionExpired),
            s if !(200..300).contains(&s) => {
                return Err(StreamableHttpError::UnexpectedServerResponse(Cow::Owned(
                    format!("HTTP {s}"),
                )))
            }
            _ => {}
        }
        if !is_request && reply.body.is_empty() {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        let ct = reply.content_type.unwrap_or_default();
        if ct.starts_with("text/event-stream") {
            Ok(StreamableHttpPostResponse::Sse(
                sse(reply.body),
                reply.session,
            ))
        } else if ct.starts_with("application/json") {
            match serde_json::from_slice::<ServerJsonRpcMessage>(&reply.body) {
                Ok(m) => Ok(StreamableHttpPostResponse::Json(m, reply.session)),
                Err(_) if !is_request => Ok(StreamableHttpPostResponse::Accepted),
                Err(_) => Err(StreamableHttpError::UnexpectedServerResponse(
                    Cow::Borrowed("the reply is not a JSON-RPC message"),
                )),
            }
        } else {
            Err(StreamableHttpError::UnexpectedContentType(Some(ct)))
        }
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        let hs = headers(
            Some(&session_id),
            auth_header.as_deref(),
            custom_headers,
            "*/*",
        );
        let (agent, cap) = (self.agent.clone(), self.cap);
        let _ = blocking(move || exchange(&agent, "DELETE", &uri, hs, None, cap)).await;
        Ok(())
    }

    async fn get_stream(
        &self,
        _uri: Arc<str>,
        _session_id: Option<Arc<str>>,
        _last_event_id: Option<String>,
        _auth_header: Option<String>,
        _custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        Err(StreamableHttpError::ServerDoesNotSupportSse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_follows_no_redirects_or_environment_proxies() {
        let client = UreqClient::new(Duration::from_secs(5), 1024);
        let config = client.agent.config();
        assert!(config.proxy().is_none());
        assert_eq!(config.max_redirects(), 0);
    }
}
