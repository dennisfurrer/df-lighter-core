//! HTTP and websocket for native programs: [`Native`] sends requests through reqwest on rustls,
//! [`clock`] is the system clock, and a [`Feed`] keeps the websocket open over tokio-tungstenite.
//! Hand the first two to a [`crate::Client`]:
//!
//! ```no_run
//! use df_lighter_core::client::native::{Native, clock};
//! use df_lighter_core::{Client, Network};
//!
//! let client = Client::reader(Native::new(), Network::Lighter, clock);
//! ```
//!
//! A [`Native`] future must be polled inside a tokio runtime. Nothing here retries.
//! [`keep_warm`] holds a client's connection open between orders, so the first one after a quiet
//! spell skips DNS and the TLS handshake.

pub mod socket;

use crate::client::{Client, Http, Method, Request, Response, TransportError};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use socket::Feed;

/// How long a request may take, connection and answer together, unless the
/// program says otherwise.
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// One HTTP client, cheap to clone: clones share their connections.
#[derive(Debug, Clone)]
pub struct Native {
    client: reqwest::Client,
}

impl Native {
    /// A client with [`TIMEOUT`].
    #[must_use]
    pub fn new() -> Self {
        Native::with_timeout(TIMEOUT)
    }

    /// A client whose every request gives up after `timeout`. A request that times out comes
    /// back as [`TransportError::Unreachable`]: a transaction may still have landed.
    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Self {
        let client = reqwest::Client::builder().timeout(timeout).build().unwrap_or_else(|_| reqwest::Client::new());
        Native { client }
    }

    /// Requests through a client the program built itself: its own user agent, pool or timeouts.
    #[must_use]
    pub const fn with_client(client: reqwest::Client) -> Self {
        Native { client }
    }
}

impl Default for Native {
    fn default() -> Self {
        Native::new()
    }
}

impl Http for Native {
    async fn send(&self, request: Request) -> Result<Response, TransportError> {
        request.refuse_if_offline()?;
        let unreachable = |error: reqwest::Error| TransportError::Unreachable(error.without_url().to_string());
        let mut outgoing = self.client.request(method(request.method), &request.url);
        for (name, value) in &request.headers {
            outgoing = outgoing.header(name, value);
        }
        if !request.body.is_empty() {
            outgoing = outgoing.body(request.body);
        }
        let answer = outgoing.send().await.map_err(unreachable)?;
        let status = answer.status().as_u16();
        let headers = answer
            .headers()
            .iter()
            .map(|(name, value)| (name.as_str().to_owned(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
            .collect();
        let body = answer.bytes().await.map_err(unreachable)?.to_vec();
        Ok(Response { status, headers, body })
    }
}

fn method(method: Method) -> reqwest::Method {
    match method {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
    }
}

/// How often [`keep_warm`] reads, inside the time a pooled connection is kept idle. On a Standard
/// account that is two of its sixty requests a minute.
pub const KEEP_WARM: Duration = Duration::from_secs(30);

/// Warms `client` every `every` while anything else holds it, and ends by itself once the last
/// holder lets go. Each warm is [`Client::warm`]: the venue's status, never signed.
pub fn keep_warm<K: Send + Sync + 'static>(
    client: &Arc<Client<Native, K>>,
    every: Duration,
) -> tokio::task::JoinHandle<()> {
    let held = Arc::downgrade(client);
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(every);
        loop {
            ticks.tick().await;
            let Some(client) = held.upgrade() else {
                return;
            };
            client.warm().await;
        }
    })
}

/// Unix milliseconds from the system clock: the [`crate::client::Clock`] for a native program. A
/// clock set before 1970 reads 0.
#[must_use]
pub fn clock() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
#[path = "tests/http.rs"]
mod tests;
