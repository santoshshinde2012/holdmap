//! Bounded authenticated metadata client for the local recording endpoint.
//!
//! This client accepts only an exact IPv4 loopback endpoint. It does not use DNS, proxies,
//! redirects or TLS and never includes response bodies or credentials in errors.

use crate::{
    credentials::{validate_credentials, CredentialError},
    protocol::Route,
    ActivitySnapshot, ActivitySpan, CollectorCredentials, COLLECTOR_LIMITS,
};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_millis(750);
const MAX_RESPONSE: usize = 2 * 1024 * 1024;
const MAX_HEADER: usize = 8192;

/// A fixed-message error that never exposes a request, response or bearer token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientError(&'static str);

impl std::fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for ClientError {}

/// A client for one explicitly supplied local recording session.
#[derive(Clone)]
pub struct LocalClient {
    address: SocketAddr,
    token: String,
}

impl LocalClient {
    /// Check an exact `http://127.0.0.1:PORT` endpoint and a 32-byte hexadecimal token.
    pub fn new(endpoint: &str, token: &str) -> Result<Self, ClientError> {
        let address = validate_credentials(endpoint, token).map_err(|error| match error {
            CredentialError::Endpoint => ClientError("Use the active local recording endpoint."),
            CredentialError::Token => ClientError("Use the active local recording token."),
        })?;
        Ok(Self {
            address,
            token: token.to_owned(),
        })
    }

    /// Connect using credentials returned by an explicit recording start.
    pub fn from_credentials(credentials: &CollectorCredentials) -> Result<Self, ClientError> {
        Self::new(&credentials.endpoint, &credentials.token)
    }

    /// Submit normalized metadata only. The collector independently validates every record.
    pub fn record(&self, spans: &[ActivitySpan]) -> Result<(), ClientError> {
        let body = serde_json::to_vec(spans)
            .map_err(|_| ClientError("Couldn't encode activity metadata."))?;
        let response = self.post(Route::Activity, &body)?;
        let result: serde_json::Value = serde_json::from_slice(&response)
            .map_err(|_| ClientError("The recording acknowledgement was invalid."))?;
        if result["accepted"].as_u64() != Some(spans.len() as u64)
            || result["dropped"].as_u64() != Some(0)
        {
            return Err(ClientError(
                "The local recorder rejected activity metadata.",
            ));
        }
        Ok(())
    }

    /// Read the current bounded metadata snapshot; credentials are never in that snapshot.
    pub fn snapshot(&self) -> Result<ActivitySnapshot, ClientError> {
        let body = self.post(Route::Snapshot, b"{}")?;
        serde_json::from_slice(&body)
            .map_err(|_| ClientError("The recording snapshot was invalid."))
    }

    fn post(&self, route: Route, body: &[u8]) -> Result<Vec<u8>, ClientError> {
        if body.len() > COLLECTOR_LIMITS.max_body_bytes {
            return Err(ClientError("Activity metadata exceeded the request limit."));
        }
        let started = Instant::now();
        let mut stream = TcpStream::connect_timeout(&self.address, TIMEOUT)
            .map_err(|_| ClientError("The local recorder is unavailable."))?;
        let path = route.path();
        let header = format!(
            "POST {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.address,
            self.token,
            body.len()
        );
        write_deadline(&mut stream, header.as_bytes(), started)?;
        write_deadline(&mut stream, body, started)?;
        let mut response = Vec::with_capacity(8192);
        let mut chunk = [0; 8192];
        loop {
            let remaining = TIMEOUT.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(ClientError("The local recorder timed out."));
            }
            stream
                .set_read_timeout(Some(remaining))
                .map_err(|_| ClientError("Couldn't read the local recorder response."))?;
            let count = stream
                .read(&mut chunk)
                .map_err(|_| ClientError("Couldn't read the local recorder response."))?;
            if count == 0 {
                break;
            }
            if response.len().saturating_add(count) > MAX_RESPONSE + MAX_HEADER {
                return Err(ClientError("The recording response exceeded its limit."));
            }
            response.extend_from_slice(&chunk[..count]);
            if let Some(result) = parse_response(&response)? {
                return Ok(result);
            }
        }
        parse_response(&response)?.ok_or(ClientError("The recording response was incomplete."))
    }
}

fn write_deadline(
    stream: &mut TcpStream,
    mut bytes: &[u8],
    started: Instant,
) -> Result<(), ClientError> {
    while !bytes.is_empty() {
        let remaining = TIMEOUT.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(ClientError("The local recorder timed out."));
        }
        stream
            .set_write_timeout(Some(remaining))
            .map_err(|_| ClientError("Couldn't contact the local recorder."))?;
        let count = stream
            .write(bytes)
            .map_err(|_| ClientError("Couldn't send activity metadata."))?;
        if count == 0 {
            return Err(ClientError("Couldn't send activity metadata."));
        }
        bytes = &bytes[count..];
    }
    Ok(())
}

fn parse_response(bytes: &[u8]) -> Result<Option<Vec<u8>>, ClientError> {
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut response = httparse::Response::new(&mut headers);
    let header_size = match response
        .parse(bytes)
        .map_err(|_| ClientError("The recording response was invalid."))?
    {
        httparse::Status::Partial => {
            if bytes.len() > MAX_HEADER {
                return Err(ClientError(
                    "The recording response header exceeded its limit.",
                ));
            }
            return Ok(None);
        }
        httparse::Status::Complete(size) if size <= MAX_HEADER => size,
        _ => {
            return Err(ClientError(
                "The recording response header exceeded its limit.",
            ))
        }
    };
    if response.code != Some(200) {
        return Err(ClientError("The local recorder rejected the request."));
    }
    let mut length = None;
    for header in response.headers.iter() {
        if header.name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(ClientError(
                "The recording response used unsupported framing.",
            ));
        }
        if header.name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err(ClientError("The recording response had ambiguous framing."));
            }
            length = std::str::from_utf8(header.value)
                .ok()
                .filter(|value| {
                    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                })
                .and_then(|value| value.parse::<usize>().ok());
            if length.is_none() {
                return Err(ClientError("The recording response had invalid framing."));
            }
        }
    }
    let length = length
        .filter(|size| *size <= MAX_RESPONSE)
        .ok_or(ClientError("The recording response had invalid framing."))?;
    let end = header_size
        .checked_add(length)
        .ok_or(ClientError("The recording response had invalid framing."))?;
    if bytes.len() < end {
        return Ok(None);
    }
    if bytes.len() != end {
        return Err(ClientError("The recording response had ambiguous framing."));
    }
    Ok(Some(bytes[header_size..end].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_refuses_dns_remote_paths_and_header_injection() {
        let token = "a".repeat(64);
        for endpoint in [
            "https://127.0.0.1:1234",
            "http://localhost:1234",
            "http://127.0.0.1:0",
            "http://127.0.0.1:1234/redirect",
            "http://127.0.0.1:1234?secret=private",
            "http://192.168.1.2:1234",
            "http://127.0.0.1:1234\r\nPrivate: value",
        ] {
            assert!(LocalClient::new(endpoint, &token).is_err());
        }
        assert!(LocalClient::new("http://127.0.0.1:1234", &token).is_ok());
        assert!(LocalClient::new("http://127.0.0.1:1234", &"\n".repeat(64)).is_err());
    }

    #[test]
    fn response_parser_rejects_ambiguous_truncated_and_large_frames() {
        assert_eq!(
            parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}").unwrap(),
            Some(b"{}".to_vec())
        );
        assert_eq!(
            parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{").unwrap(),
            None
        );
        for response in [
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}",
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 3000000\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}trailing",
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 13\r\n\r\nprivate-token",
        ] {
            let error = parse_response(response.as_bytes()).unwrap_err().to_string();
            assert!(!error.contains("private-token"));
        }
    }
}
