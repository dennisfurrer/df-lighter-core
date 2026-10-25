//! A [`Client`] on either venue, and a node on either L1, whose wire paces every request, lets
//! only reads out and keeps every answer, so a test can hold what was said against the recording
//! the unit and mocked suites read.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use df_lighter_core::client::evm::Rpc;
use df_lighter_core::client::native::{Native, clock};
use df_lighter_core::client::{Http, Method, Request, Response, TransportError};
use df_lighter_core::utils::Address;
use df_lighter_core::{Client, Network};
use serde_json::Value;

use crate::shape::drift;
use crate::support::fixture;

/// The least time between two requests of this suite, to any host: a Standard account may send
/// sixty a minute from one IP, and this sends at most forty.
pub const GAP: Duration = Duration::from_millis(1_500);

/// When the next request may go out, shared by every test.
static NEXT: Mutex<Option<Instant>> = Mutex::new(None);

/// 0x4786...F91A, which owns account 10 on Lighter.
pub fn owner() -> Address {
    "0x478686468057D9873E8718083D40d2927b12F91A".parse().unwrap()
}

/// An address neither venue nor either L1 contract has seen.
pub fn unused() -> Address {
    "0x5b1c8f4a0e7d2c93b6a1f0e4d8c7b2a9e3f6d104".parse().unwrap()
}

/// The real wire: each request waits its turn and must be a read; each answer's text is kept.
pub struct Tap {
    wire: Native,
    kept: RefCell<Vec<(String, String)>>,
}

impl Tap {
    fn new() -> Tap {
        Tap { wire: Native::new(), kept: RefCell::default() }
    }

    /// How many requests were answered.
    pub fn count(&self) -> usize {
        self.kept.borrow().len()
    }

    /// The last answer, as JSON.
    pub fn last(&self) -> Value {
        let kept = self.kept.borrow();
        let (url, answer) = kept.last().expect("nothing was sent");
        serde_json::from_str(answer)
            .unwrap_or_else(|error| panic!("{error}: the answer from {url} is not JSON: {answer}"))
    }
}

/// A read that got no answer at all is asked once more, in its next turn: every request here is
/// a read, so asking twice changes nothing.
impl Http for Tap {
    async fn send(&self, request: Request) -> Result<Response, TransportError> {
        read_only(&request);
        let url = request.url.clone();
        turn().await;
        let answer = match self.wire.send(request.clone()).await {
            Err(TransportError::Unreachable(_)) => {
                turn().await;
                self.wire.send(request).await?
            }
            answer => answer?,
        };
        self.kept.borrow_mut().push((url, answer.text()));
        Ok(answer)
    }
}

/// Only reads leave: a GET to a venue, or `eth_chainId` or `eth_call` posted to an L1's public
/// node. Anything else fails the test before it goes out.
fn read_only(request: &Request) {
    assert!(!request.is_signed(), "the live suite sends nothing signed: {}", request.url);
    if request.method == Method::Post {
        let body = String::from_utf8_lossy(&request.body);
        let node = Network::ALL.iter().any(|network| request.url == network.l1_rpc_url());
        let read = [r#""method":"eth_chainId""#, r#""method":"eth_call""#].iter().any(|method| body.contains(method));
        assert!(node && read, "the live suite posts nothing but L1 reads: {} {body}", request.url);
    }
}

/// Waits until [`GAP`] after the request before.
async fn turn() {
    let at = {
        let mut next = NEXT.lock().unwrap_or_else(PoisonError::into_inner);
        let at = next.map_or_else(Instant::now, |next| next.max(Instant::now()));
        *next = Some(at + GAP);
        at
    };
    tokio::time::sleep_until(at.into()).await;
}

/// A reader on `network` through a [`Tap`], on the Standard tier.
pub fn reader(network: Network) -> Client<Tap> {
    Client::reader(Tap::new(), network, clock)
}

/// The public node on `network`'s L1 through a [`Tap`].
pub fn node(network: Network) -> Rpc<Tap> {
    Rpc::l1(Tap::new(), network)
}

/// What sends through a [`Tap`]: a client or a node.
pub trait Tapped {
    fn tap(&self) -> &Tap;
}

impl Tapped for Client<Tap> {
    fn tap(&self) -> &Tap {
        self.http()
    }
}

impl Tapped for Rpc<Tap> {
    fn tap(&self) -> &Tap {
        self.http()
    }
}

/// `network`'s recording `name`: `<network>/<name>` under `fixtures/`.
pub fn recording(network: Network, name: &str) -> String {
    format!("{}/{name}", network.name())
}

/// Holds the last answer against the recording `fixtures/<path>`. Fails naming every key path
/// the venue has added since and every value of a kind the recording never had there, and keeps
/// the live answer as `target/tmp/live/<path>` to compare with.
pub fn same_shape(sender: &impl Tapped, path: &str) {
    same_shape_as_any(sender, &[path], &[]);
}

/// [`same_shape`] against several recordings of rows of one kind, for fields the venue writes
/// on some rows only: a path drifts only when none of them has it, and never when it is one of
/// `documented`, fields the docs list that no recording caught.
pub fn same_shape_as_any(sender: &impl Tapped, paths: &[&str], documented: &[&str]) {
    let live = sender.tap().last();
    keep(paths[0], &live);
    let recorded: Vec<Value> = paths.iter().map(|path| serde_json::from_str(&fixture(path)).unwrap()).collect();
    let drift: Vec<String> =
        drift(&recorded, &live).into_iter().filter(|path| !documented.contains(&path.as_str())).collect();
    assert!(drift.is_empty(), "the answer has drifted from {paths:?}: {drift:#?}");
}

fn keep(path: &str, live: &Value) {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("live").join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_string_pretty(live).unwrap()).unwrap();
}
