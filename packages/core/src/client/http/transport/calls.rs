//! The [`Transport`] every read and send goes through, the [`Call`] it carries (a GET with a query
//! or a form POST under `/api/v1`, with or without the account's auth token), and reading the
//! venue's answer: the value asked for, or its refusal by code.

use std::future::Future;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::client::http::transport::wire::{Method, TransportError};

/// Where the REST API lives under a network's host.
pub const API_PREFIX: &str = "/api/v1/";

/// One request to the venue's REST API: its method, its path under `/api/v1/` (or from the root,
/// `/info`, when it starts with `/`), its fields in order, and whether it is the account's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub method: Method,
    pub path: &'static str,
    pub fields: Vec<(&'static str, String)>,
    /// The read or post is an account's own: it carries the `Authorization` header.
    pub auth: bool,
    /// A JSON body posted in place of the form, for the one post that takes one
    /// (`setAccountMetadata`).
    pub json: Option<String>,
}

impl Call {
    /// A GET; its fields go in the query string.
    #[must_use]
    pub const fn get(path: &'static str) -> Call {
        Call { method: Method::Get, path, fields: Vec::new(), auth: false, json: None }
    }

    /// A form POST; its fields go in the body.
    #[must_use]
    pub const fn post(path: &'static str) -> Call {
        Call { method: Method::Post, path, fields: Vec::new(), auth: false, json: None }
    }

    /// One more field.
    #[must_use]
    pub fn field(mut self, key: &'static str, value: impl ToString) -> Call {
        self.fields.push((key, value.to_string()));
        self
    }

    /// One more field when there is a value.
    #[must_use]
    pub fn maybe(self, key: &'static str, value: Option<impl ToString>) -> Call {
        match value {
            Some(value) => self.field(key, value),
            None => self,
        }
    }

    /// The same post with `body` as its JSON in place of a form.
    #[must_use]
    pub fn with_json(mut self, body: String) -> Call {
        self.json = Some(body);
        self
    }

    /// The account's own: sent with its auth token.
    #[must_use]
    pub const fn authed(mut self) -> Call {
        self.auth = true;
        self
    }

    /// The path from the host: `/api/v1/orderBookDetails`, or `/info`.
    #[must_use]
    pub fn url_path(&self) -> String {
        match self.path.starts_with('/') {
            true => self.path.to_owned(),
            false => format!("{API_PREFIX}{}", self.path),
        }
    }

    /// The field a value was given for.
    #[must_use]
    pub fn value(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|(name, _)| *name == key).map(|(_, value)| value.as_str())
    }
}

/// Sends a [`Call`] to one network and returns the answer's text, whatever its `code`.
/// [`crate::client::Client`] implements it behind the rate limit, adding the auth token a call
/// asks for. No `Send` bound, since the browser is single-threaded.
pub trait Transport {
    fn call(&self, call: Call) -> impl Future<Output = Result<String, TransportError>>;
}

/// Why a read or a send came to nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("the venue's answer on {path} could not be read: {detail}")]
    Unreadable { path: String, detail: String },
    /// The venue's own `{"code":..,"message":..}`: the code says why ([`ApiError::code`]), the
    /// message is shown as it came.
    #[error("refused ({code}): {message}")]
    Refused { code: i64, message: String },
}

/// Codes worth telling apart, from the venue's list.
pub mod codes {
    pub const OK: i64 = 200;
    pub const INVALID_PARAM: i64 = 20001;
    pub const INVALID_AUTH: i64 = 20013;
    pub const ACCOUNT_NOT_FOUND: i64 = 21100;
    pub const INVALID_NONCE: i64 = 21104;
    pub const NON_INCREASING_NONCE: i64 = 21105;
    pub const INVALID_PUBLIC_KEY: i64 = 21108;
    pub const API_KEY_NOT_FOUND: i64 = 21109;
    pub const INVALID_SIGNATURE: i64 = 21120;
    pub const ZERO_COLLATERAL_CHANGE_PUB_KEY: i64 = 21126;
    pub const INVALID_FEE: i64 = 21134;
    pub const PUBLIC_KEY_UPDATE_SDK: i64 = 21136;
    pub const TOO_MANY_PENDING: i64 = 21506;
    pub const BELOW_MAINTENANCE_MARGIN: i64 = 21507;
    pub const BELOW_INITIAL_MARGIN: i64 = 21508;
    pub const NOT_ENOUGH_COLLATERAL: i64 = 21301;
    pub const NOT_ENOUGH_ORDER_MARGIN: i64 = 21739;
    pub const CLIENT_ORDER_INDEX_EXISTS: i64 = 21728;
    pub const TOO_MANY_REQUESTS: i64 = 23000;
    /// The L1 address has made its fast withdrawals for the hour.
    pub const TOO_MANY_FAST_WITHDRAWALS: i64 = 23005;
    pub const INTERNAL: i64 = 29500;
    /// `account` by an index no account has: with a 400, where a read by address answers
    /// [`ACCOUNT_NOT_FOUND`].
    pub const NOT_FOUND: i64 = 29404;
}

impl ApiError {
    /// The venue's code, when it gave one.
    #[must_use]
    pub const fn code(&self) -> Option<i64> {
        match self {
            ApiError::Refused { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// Refused for the rate limit, here or by the venue: wait and ask again.
    #[must_use]
    pub const fn is_rate_limited(&self) -> bool {
        matches!(self, ApiError::Transport(TransportError::RateLimited { .. }))
            || matches!(self.code(), Some(codes::TOO_MANY_REQUESTS | codes::TOO_MANY_PENDING))
    }

    /// The nonce was refused: the key's count is off, so ask `nextNonce` and sign again.
    #[must_use]
    pub const fn is_nonce(&self) -> bool {
        matches!(self.code(), Some(codes::INVALID_NONCE | codes::NON_INCREASING_NONCE))
    }

    /// The API key is not, or no longer, the one registered at its slot.
    #[must_use]
    pub const fn is_unregistered_key(&self) -> bool {
        matches!(
            self.code(),
            Some(
                codes::INVALID_PUBLIC_KEY
                    | codes::API_KEY_NOT_FOUND
                    | codes::INVALID_SIGNATURE
                    | codes::PUBLIC_KEY_UPDATE_SDK
            )
        )
    }

    /// Whether a transaction this answered may have been taken: no answer, an unreadable one, or
    /// the venue's own internal error. A refusal with any other code, or a rate limit, was turned
    /// away before it counted.
    #[must_use]
    pub const fn may_have_landed(&self) -> bool {
        match self {
            ApiError::Transport(TransportError::Unreachable(_)) | ApiError::Unreadable { .. } => true,
            ApiError::Transport(TransportError::Status { status, .. }) => *status >= 500,
            ApiError::Transport(_) => false,
            ApiError::Refused { code, .. } => *code == codes::INTERNAL,
        }
    }
}

/// `code` and `message`, which every answer may carry.
#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    message: Option<String>,
}

/// One read, not retried: the caller decides.
pub async fn read<R: DeserializeOwned>(transport: &impl Transport, call: Call) -> Result<R, ApiError> {
    let path = call.url_path();
    match transport.call(call).await {
        Ok(text) => decode(&path, &text),
        Err(TransportError::Status { status, body }) => match refusal(&body) {
            Some(refused) => Err(refused),
            None => Err(ApiError::Transport(TransportError::Status { status, body })),
        },
        Err(error) => Err(ApiError::Transport(error)),
    }
}

/// An answer's text read as `R`, or the venue's refusal: a `code` other than 200 (or 0).
pub fn decode<R: DeserializeOwned>(path: &str, text: &str) -> Result<R, ApiError> {
    if let Some(refused) = refusal(text) {
        return Err(refused);
    }
    serde_json::from_str(text)
        .map_err(|error| ApiError::Unreadable { path: path.to_owned(), detail: unreadable(&error, text) })
}

/// The refusal an answer carries, if it is one.
fn refusal(text: &str) -> Option<ApiError> {
    let envelope = serde_json::from_str::<Envelope>(text).ok()?;
    match envelope.code {
        Some(0 | codes::OK) | None => None,
        Some(code) => Some(ApiError::Refused { code, message: envelope.message.unwrap_or_default() }),
    }
}

pub(crate) fn unreadable(error: &serde_json::Error, text: &str) -> String {
    let shown: String = text.chars().take(300).collect();
    format!("{error} in {shown}")
}

#[cfg(test)]
#[path = "tests/calls.rs"]
mod tests;
