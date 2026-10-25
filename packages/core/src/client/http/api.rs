//! One network's REST API behind one rate-limit gate, the part of a [`crate::client::Client`] that
//! calls out. Every read and send passes the gate and the meter; the [`Http`] underneath knows
//! nothing of networks.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::client::Clock;
use crate::client::http::limit::{Gate, Meter, Spent, Tier, is_send, weight};
use crate::client::http::transport::{Call, Http, Method, Request, TransportError, form};
use crate::client::network::Network;

#[derive(Debug, Default)]
struct Limiter {
    gate: Gate,
    meter: Meter,
}

/// One network's REST API, rate limited for one account tier.
pub(crate) struct Api<H> {
    http: H,
    network: Network,
    tier: Tier,
    clock: Clock,
    limiter: Mutex<Limiter>,
}

impl<H: Http> Api<H> {
    pub(crate) fn new(http: H, network: Network, clock: Clock) -> Self {
        Api { http, network, tier: Tier::Standard, clock, limiter: Mutex::new(Limiter::default()) }
    }

    pub(crate) fn with_tier(mut self, tier: Tier) -> Self {
        self.tier = tier;
        self
    }

    #[must_use]
    pub(crate) const fn network(&self) -> Network {
        self.network
    }

    #[must_use]
    pub(crate) const fn tier(&self) -> Tier {
        self.tier
    }

    #[must_use]
    pub(crate) const fn http(&self) -> &H {
        &self.http
    }

    #[must_use]
    pub(crate) const fn clock(&self) -> Clock {
        self.clock
    }

    /// What went out over the last minute, and how long the gate stays closed.
    pub(crate) fn spent(&self) -> Spent {
        let now = (self.clock)();
        let mut limiter = self.limiter();
        let cooldown_ms = limiter.gate.cooldown_ms(now);
        Spent { cooldown_ms, ..limiter.meter.spent(now, self.tier.budget(self.network)) }
    }

    fn limiter(&self) -> MutexGuard<'_, Limiter> {
        self.limiter.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The request a call makes on this network, with `token` as its `Authorization`.
    pub(crate) fn request(&self, call: &Call, token: Option<&str>) -> Request {
        let url = format!("{}{}", self.network.api_url(), call.url_path());
        let request = match call.method {
            Method::Get if call.fields.is_empty() => Request::get(url),
            Method::Get => Request::get(format!("{url}?{}", form(&call.fields))),
            Method::Post => match &call.json {
                Some(body) => Request::post_json(url, body.clone()),
                None => Request::post_form(url, &call.fields),
            },
        };
        match token {
            Some(token) => request.with_header("Authorization", token),
            None => request,
        }
    }

    /// Past the gate, then to the network. A read near the budget is held back as
    /// [`TransportError::RateLimited`] without going out; a send never is, but a closed gate
    /// holds both. Nothing is retried.
    pub(crate) async fn call(&self, call: &Call, token: Option<&str>) -> Result<String, TransportError> {
        let send = is_send(call.path);
        let cost = self.tier.cost(weight(self.network, call.path));
        let budget = self.tier.budget(self.network);
        let ticket = {
            let now = (self.clock)();
            let mut limiter = self.limiter();
            let ticket = limiter.gate.open(now).map_err(|wait| rate_limited(Some(wait)))?;
            let held = !send || self.tier.sends_share_reads();
            if held && let Some(wait) = limiter.meter.wait_for(now, cost, budget) {
                return Err(rate_limited(Some(wait)));
            }
            ticket
        };
        let answer = self.http.send(self.request(call, token)).await;
        let now = (self.clock)();
        let mut limiter = self.limiter();
        let response = answer?;
        match response.status {
            429 => {
                limiter.meter.refuse(now);
                let wait = limiter.gate.struck(ticket, now, response.retry_after_ms());
                return Err(rate_limited(Some(wait)));
            }
            405 => {
                limiter.meter.refuse(now);
                let wait = limiter.gate.blocked(ticket, now);
                return Err(rate_limited(Some(wait)));
            }
            _ => {}
        }
        if !send || self.tier.sends_share_reads() {
            limiter.meter.record(now, call.path, cost);
        }
        let text = response.text();
        match response.is_success() {
            true => {
                limiter.gate.passed(ticket);
                Ok(text)
            }
            false => Err(TransportError::Status { status: response.status, body: text }),
        }
    }
}

const fn rate_limited(retry_after_ms: Option<u64>) -> TransportError {
    TransportError::RateLimited { retry_after_ms }
}

#[cfg(test)]
#[path = "tests/api.rs"]
mod tests;
