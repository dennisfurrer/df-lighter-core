//! The [`Http`] wire: one request out, with its method, headers and body, and
//! its answer back whatever the status, and the form encoding Lighter's posts take. A host or
//! a dependent plugs in here.

use std::future::Future;

/// One HTTP request out and its answer back, whatever the status. It knows
/// nothing of networks or limits; an error means no answer at all
/// ([`TransportError::Unreachable`]).
pub trait Http {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    #[must_use]
    pub fn get(url: impl Into<String>) -> Self {
        Request { method: Method::Get, url: url.into(), headers: Vec::new(), body: Vec::new() }
    }

    /// A JSON body, posted as `application/json`: what a JSON-RPC node and the bridge take.
    #[must_use]
    pub fn post_json(url: impl Into<String>, body: String) -> Self {
        Request {
            method: Method::Post,
            url: url.into(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: body.into_bytes(),
        }
    }

    /// Fields posted as `application/x-www-form-urlencoded`, in order: what every POST under
    /// `/api/v1` takes, `sendTx` among them.
    #[must_use]
    pub fn post_form(url: impl Into<String>, fields: &[(&str, String)]) -> Self {
        Request {
            method: Method::Post,
            url: url.into(),
            headers: vec![("Content-Type".to_owned(), "application/x-www-form-urlencoded".to_owned())],
            body: form(fields).into_bytes(),
        }
    }

    /// The same request with one more header.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// A header's value, its name matched whatever its case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }

    /// Whether it carries a signature: a post that sends a transaction (`sendTx`, `sendTxBatch`,
    /// a fast withdrawal, a LIT lease), or a raw EVM transaction.
    #[must_use]
    pub fn is_signed(&self) -> bool {
        const RAW: &[u8] = b"eth_sendRawTransaction";
        let path = self.url.split(['?', '#']).next().unwrap_or_default().trim_end_matches('/');
        self.method == Method::Post
            && (SIGNED_PATHS.iter().any(|signed| path.ends_with(signed))
                || self.body.windows(RAW.len()).any(|window| window == RAW))
    }

    /// [`TransportError::Offline`] for a signed request in an `offline` build; otherwise nothing.
    pub fn refuse_if_offline(&self) -> Result<(), TransportError> {
        match OFFLINE && self.is_signed() {
            true => Err(TransportError::Offline(self.url.clone())),
            false => Ok(()),
        }
    }
}

/// The posts under `/api/v1` that carry a signed transaction.
const SIGNED_PATHS: [&str; 4] = ["/api/v1/sendTx", "/api/v1/sendTxBatch", "/api/v1/fastwithdraw", "/api/v1/litLease"];

/// `application/x-www-form-urlencoded`, as `URLSearchParams` writes it: letters, digits and `*-._`
/// as they are, a space as `+`, every other byte `%XX`. A query string is written the same way.
#[must_use]
pub fn form(fields: &[(&str, String)]) -> String {
    let mut text = String::new();
    for (at, (key, value)) in fields.iter().enumerate() {
        if at > 0 {
            text.push('&');
        }
        escape(key, &mut text);
        text.push('=');
        escape(value, &mut text);
    }
    text
}

fn escape(value: &str, out: &mut String) {
    for byte in value.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => out.push(char::from(byte)),
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[usize::from(byte >> 4)]));
                out.push(char::from(b"0123456789ABCDEF"[usize::from(byte & 0x0f)]));
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// A header's value, its name matched whatever its case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }

    /// `Retry-After` in milliseconds, when it is given in whole seconds.
    #[must_use]
    pub fn retry_after_ms(&self) -> Option<u64> {
        let seconds = self.header("retry-after").and_then(|seconds| seconds.trim().parse::<u64>().ok())?;
        Some(seconds.saturating_mul(1_000))
    }

    /// The body as text; bytes that are not UTF-8 read as U+FFFD.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// No answer came back. For a transaction that means unknown, not failed: it may have
    /// landed, so it is never sent again; read the account or the transaction to find out.
    #[error("no answer from the venue: {0}")]
    Unreachable(String),
    #[error("the venue answered HTTP {status}: {body}")]
    Status { status: u16, body: String },
    /// Refused for the rate limit, by the venue (a 429, or the 405 its firewall answers) or by
    /// the [`crate::client::Client`]'s gate before it went out. Nothing was done; try again after
    /// the wait.
    #[error("rate limited; try again in {} ms", retry_after_ms.unwrap_or(0))]
    RateLimited { retry_after_ms: Option<u64> },
    /// Not sent: this build has the `offline` feature, and the request carried a signature.
    #[error("an offline build sends nothing signed: {0}")]
    Offline(String),
    /// Not sent: the read is an account's own and this client holds neither an API key to mint
    /// an auth token with nor a read-only token.
    #[error("{0} needs an auth token: give the client an API key or a read-only token")]
    NoToken(String),
}

/// Whether this build's transports refuse what is signed ([`TransportError::Offline`]).
pub const OFFLINE: bool = cfg!(feature = "offline");

#[cfg(test)]
#[path = "tests/wire.rs"]
mod tests;
