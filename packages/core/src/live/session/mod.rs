//! One connection's state with no socket and no clock: the socket's owner hands in what happened
//! and when, and sends what comes back. It keeps what is wanted and subscribes it on every
//! socket, with the account's auth token where a channel takes it; checks each order book's nonce
//! chain and, on a gap, owes the venue the resubscribe that brings a fresh snapshot; answers the
//! venue's pings; and sends signed transactions, matching each answer to its id. Of the venue's
//! limits, a connection's subscriptions and an IP's accounts are refused here; connections and
//! message rates are the owner's to keep.
//!
//! - [`limits`]: the venue's limits and why a session refuses.
//! - [`timing`]: the ping, the silence that ends a socket, the reconnect delays.

pub mod limits;
pub mod timing;

pub use limits::{
    Limits, MAX_ACCOUNTS, MAX_CONNECTIONS, MAX_INFLIGHT_MESSAGES, MAX_MESSAGES_PER_MINUTE, MAX_SUBSCRIPTIONS,
    MAX_SUBSCRIPTIONS_PER_CONNECTION, Refused,
};
pub use timing::{Retry, Tick, Timing};

use std::collections::BTreeMap;
use std::fmt;

use crate::client::Network;
use crate::live::{
    BadFrame, Data, Frame, Gap, PONG, Subscription, decode, request::batch_fits, resubscribe, send_tx, send_tx_batch,
    subscribe, unsubscribe,
};
use crate::utils::MarketId;
use crate::utils::sign::Signed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    Closed,
    Open {
        /// When it opened.
        since: u64,
        /// When anything last arrived; `since` until something does.
        heard: u64,
        /// Whether anything has arrived at all.
        talked: bool,
        /// When the last ping went out; `since` until one does.
        pinged: u64,
    },
}

/// The account's auth token: shown to no one, in a log or otherwise.
#[derive(Clone, PartialEq, Eq)]
struct Token(String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(..)")
    }
}

/// One connection to one network's websocket.
#[derive(Debug, Clone)]
pub struct Session {
    network: Network,
    readonly: bool,
    limits: Limits,
    timing: Timing,
    token: Option<Token>,
    wanted: Vec<Subscription>,
    link: Link,
    /// Reconnects since a socket last worked.
    failures: u32,
    next_id: u64,
    waiting: Vec<u64>,
    /// Each book's last nonce on this socket; `None` while a fresh snapshot is awaited.
    chains: BTreeMap<MarketId, Option<u64>>,
    /// Frames the venue is owed: a pong, a resubscribe after a gap.
    owed: Vec<String>,
}

impl Session {
    /// A closed session on `network`'s socket, with the venue's limits and the default timing.
    #[must_use]
    pub fn new(network: Network) -> Self {
        Session {
            network,
            readonly: false,
            limits: Limits::VENUE,
            timing: Timing::DEFAULT,
            token: None,
            wanted: Vec::new(),
            link: Link::Closed,
            failures: 0,
            next_id: 1,
            waiting: Vec::new(),
            chains: BTreeMap::new(),
            owed: Vec::new(),
        }
    }

    /// On the read-only socket (`?readonly=true`), which serves every channel, private ones with
    /// their token included, from restricted regions too, and takes no transactions.
    #[must_use]
    pub const fn read_only(mut self) -> Self {
        self.readonly = true;
        self
    }

    /// A connection's share of the limits; sessions sharing an IP split its accounts.
    #[must_use]
    pub const fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    #[must_use]
    pub const fn with_timing(mut self, timing: Timing) -> Self {
        self.timing = timing;
        self
    }

    /// With the account's auth token ([`crate::Client::auth_token`]) for the channels that take
    /// it. A token reads one account and lapses (eight hours at most): set a fresh one before it
    /// does with [`Session::set_auth`].
    #[must_use]
    pub fn with_auth(mut self, token: impl Into<String>) -> Self {
        self.token = Some(Token(token.into()));
        self
    }

    /// A fresh token, or none. The venue reads a token when a channel is subscribed: what is
    /// already subscribed goes on, and the new one goes out with the next subscribe, resubscribe
    /// or reconnect.
    pub fn set_auth(&mut self, token: Option<String>) {
        self.token = token.map(Token);
    }

    #[must_use]
    pub const fn network(&self) -> Network {
        self.network
    }

    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.readonly
    }

    /// Where to connect: the network's socket, or its read-only one.
    #[must_use]
    pub const fn url(&self) -> &'static str {
        match self.readonly {
            true => self.network.ws_readonly_url(),
            false => self.network.ws_url(),
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        matches!(self.link, Link::Open { .. })
    }

    /// What this session subscribes to on every socket, in the order it was asked for.
    #[must_use]
    pub fn wanted(&self) -> &[Subscription] {
        &self.wanted
    }

    /// The distinct accounts the wanted subscriptions follow.
    #[must_use]
    pub fn accounts(&self) -> usize {
        let mut accounts: Vec<_> = self.wanted.iter().filter_map(Subscription::account).collect();
        accounts.sort_unstable();
        accounts.dedup();
        accounts.len()
    }

    /// The ids of transactions sent on this socket and not yet answered.
    #[must_use]
    pub fn waiting(&self) -> &[u64] {
        &self.waiting
    }

    /// Wants `subscription` from now on, on this socket and every later one. Gives the frame to
    /// send when the socket is open and it is new; nothing when it was already wanted (the venue
    /// refuses a second subscribe). Past the limits, without the token it needs, or on a network
    /// that does not serve it, it is refused and not kept.
    pub fn subscribe(&mut self, subscription: Subscription) -> Result<Option<String>, Refused> {
        if self.wanted.contains(&subscription) {
            return Ok(None);
        }
        subscription.check(self.network)?;
        if subscription.needs_auth() && self.token.is_none() {
            return Err(Refused::NoToken(subscription.channel()));
        }
        if self.wanted.len() >= self.limits.subscriptions {
            return Err(Refused::Subscriptions { limit: self.limits.subscriptions });
        }
        if let Some(account) = subscription.account()
            && !self.wanted.iter().any(|wanted| wanted.account() == Some(account))
            && self.accounts() >= self.limits.accounts
        {
            return Err(Refused::Accounts { limit: self.limits.accounts });
        }
        let frame = self.is_open().then(|| subscribe(&subscription, self.auth()));
        self.wanted.push(subscription);
        Ok(frame)
    }

    /// Stops wanting `subscription`. Gives the frame to send when the socket is open and it was
    /// wanted.
    pub fn unsubscribe(&mut self, subscription: &Subscription) -> Option<String> {
        let at = self.wanted.iter().position(|wanted| wanted == subscription)?;
        let gone = self.wanted.remove(at);
        if let Subscription::OrderBook { market } = gone {
            self.chains.remove(&market);
        }
        self.is_open().then(|| unsubscribe(&gone))
    }

    /// An unsubscribe and a subscribe for a fresh snapshot of what is wanted, on an open socket;
    /// nothing otherwise. A second subscribe alone the venue refuses.
    pub fn resubscribe(&mut self, subscription: &Subscription) -> Vec<String> {
        if !self.is_open() || !self.wanted.contains(subscription) {
            return Vec::new();
        }
        if let Subscription::OrderBook { market } = subscription {
            self.chains.insert(*market, None);
        }
        resubscribe(subscription, self.auth()).into()
    }

    /// A signed transaction's frame (`jsonapi/sendtx`) and the id its answer will name. Only on
    /// an open socket that is not read-only, and only while fewer than the limit wait for an
    /// answer. One with the owner's signature: [`Session::send_tx_info`] with
    /// [`Signed::tx_info_with_l1`].
    pub fn send_tx(&mut self, signed: &Signed) -> Result<(u64, String), Refused> {
        self.send_tx_info(signed.tx_type().code(), &signed.tx_info())
    }

    /// [`Session::send_tx`] for a type and `tx_info` as signed.
    pub fn send_tx_info(&mut self, tx_type: u8, tx_info: &str) -> Result<(u64, String), Refused> {
        let id = self.take_id()?;
        let frame = send_tx(&id.to_string(), tx_type, tx_info).map_err(Refused::Unwritable)?;
        Ok((self.sent(id), frame))
    }

    /// Up to fifty signed transactions as one batch (`jsonapi/sendtxbatch`): one account and API
    /// key, consecutive nonces. None may need the owner's signature.
    pub fn send_tx_batch(&mut self, signed: &[Signed]) -> Result<(u64, String), Refused> {
        let txs: Vec<(u8, String)> = signed.iter().map(|one| (one.tx_type().code(), one.tx_info())).collect();
        self.send_tx_batch_infos(&txs)
    }

    /// [`Session::send_tx_batch`] for types and `tx_info`s as signed.
    pub fn send_tx_batch_infos(&mut self, txs: &[(u8, String)]) -> Result<(u64, String), Refused> {
        if !batch_fits(txs.len()) {
            return Err(Refused::Batch(txs.len()));
        }
        let id = self.take_id()?;
        let frame = send_tx_batch(&id.to_string(), txs);
        Ok((self.sent(id), frame))
    }

    /// The socket opened at `now`. Gives the frames that subscribe to everything wanted, in
    /// order. Each book's chain starts again from its snapshot.
    pub fn opened(&mut self, now: u64) -> Vec<String> {
        self.link = Link::Open { since: now, heard: now, talked: false, pinged: now };
        self.chains.clear();
        self.owed.clear();
        self.wanted.iter().map(|subscription| subscribe(subscription, self.auth())).collect()
    }

    /// A text frame arrived at `now`. Anything that arrives shows the socket is alive, whether it
    /// reads or not. A transaction's answer or refusal stops it waiting; a ping is owed a pong;
    /// an order book update that does not follow its book comes back as [`Frame::Gap`], and the
    /// resubscribe that brings a fresh snapshot is owed ([`Session::owed`]).
    pub fn received(&mut self, now: u64, text: &str) -> Result<Frame, BadFrame> {
        let frame = decode(text);
        if let Link::Open { heard, talked, .. } = &mut self.link {
            *heard = (*heard).max(now);
            *talked = true;
        }
        let frame = frame?;
        if let Some(id) = frame.answers().and_then(|id| id.parse::<u64>().ok()) {
            self.waiting.retain(|waiting| *waiting != id);
        }
        Ok(match frame {
            Frame::Ping => {
                self.owed.push(PONG.to_owned());
                Frame::Ping
            }
            Frame::Snapshot(Data::OrderBook(book)) => {
                self.chains.insert(book.market, Some(book.nonce));
                Frame::Snapshot(Data::OrderBook(book))
            }
            Frame::Update(Data::OrderBook(book)) => match self.chains.get(&book.market).copied() {
                Some(Some(expected)) if book.begin_nonce != expected => {
                    let gap = Gap { market: book.market, expected, got: book.begin_nonce };
                    let again = self.resubscribe(&Subscription::OrderBook { market: book.market });
                    self.owed.extend(again);
                    self.chains.insert(book.market, None);
                    Frame::Gap(gap)
                }
                Some(Some(_)) => {
                    self.chains.insert(book.market, Some(book.nonce));
                    Frame::Update(Data::OrderBook(book))
                }
                _ => Frame::Update(Data::OrderBook(book)),
            },
            other => other,
        })
    }

    /// The frames the venue is owed, to send now and once: the pong to its ping, the
    /// unsubscribe and subscribe that bring a book a fresh snapshot after a gap.
    pub fn owed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.owed)
    }

    fn auth(&self) -> Option<&str> {
        self.token.as_ref().map(|token| token.0.as_str())
    }

    /// The next id, when a transaction may go out at all.
    fn take_id(&mut self) -> Result<u64, Refused> {
        if self.readonly {
            return Err(Refused::ReadOnly);
        }
        if !self.is_open() {
            return Err(Refused::Closed);
        }
        if self.waiting.len() >= self.limits.transactions {
            return Err(Refused::Transactions { limit: self.limits.transactions });
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        Ok(id)
    }

    /// `id` went out: it waits for its answer.
    fn sent(&mut self, id: u64) -> u64 {
        self.waiting.push(id);
        id
    }
}

#[cfg(test)]
#[path = "tests/session.rs"]
mod tests;
