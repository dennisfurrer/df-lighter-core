//! What the unit tests share: running a future without a runtime, a [`Transport`] that answers
//! from a recording, an [`Http`] wire that answers from a queue, and lighter-go's vectors. Nothing
//! leaves the process.

use std::{
    cell::RefCell,
    collections::VecDeque,
    future::{Future, ready},
    pin::pin,
    task::{Context, Poll, Waker},
};

use serde_json::Value;

use crate::client::{Call, Http, Request, Response, Transport, TransportError};
use crate::utils::sign::SecretKey;
use crate::utils::sign::tx::Envelope;
use crate::utils::{AccountIndex, ApiKeyIndex};

/// Runs a future to its end on this thread. The fakes here never wait, so each future is ready
/// on its first poll.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

/// A [`Transport`] that answers every call the same way and keeps each one.
pub(crate) struct Recorded {
    answer: Result<String, TransportError>,
    sent: RefCell<Vec<Call>>,
}

impl Recorded {
    pub(crate) fn answering(text: &str) -> Recorded {
        Recorded { answer: Ok(text.to_owned()), sent: RefCell::default() }
    }

    pub(crate) fn failing(error: TransportError) -> Recorded {
        Recorded { answer: Err(error), sent: RefCell::default() }
    }

    /// Each call, in order.
    pub(crate) fn sent(&self) -> Vec<Call> {
        self.sent.borrow().clone()
    }

    /// The one call made.
    pub(crate) fn only(&self) -> Call {
        let sent = self.sent();
        assert_eq!(sent.len(), 1, "one call: {sent:?}");
        sent.into_iter().next().unwrap_or_else(|| Call::get("none"))
    }
}

impl Transport for Recorded {
    fn call(&self, call: Call) -> impl Future<Output = Result<String, TransportError>> {
        self.sent.borrow_mut().push(call);
        ready(self.answer.clone())
    }
}

/// An [`Http`] wire that answers from a queue, then with its fallback, and keeps every request.
pub(crate) struct Wire {
    answers: RefCell<VecDeque<Result<Response, TransportError>>>,
    otherwise: Result<Response, TransportError>,
    sent: RefCell<Vec<Request>>,
}

impl Wire {
    /// Answers with `answers` in turn, then as unreachable.
    pub(crate) fn answering(answers: impl IntoIterator<Item = Result<Response, TransportError>>) -> Wire {
        Wire {
            answers: RefCell::new(answers.into_iter().collect()),
            otherwise: Err(TransportError::Unreachable("dry".to_owned())),
            sent: RefCell::default(),
        }
    }

    /// Answers every request `200` with `text`.
    pub(crate) fn always(text: &str) -> Wire {
        Wire::answering([]).then(ok(text))
    }

    /// The same wire, answering `otherwise` once its queue is empty.
    pub(crate) fn then(self, otherwise: Result<Response, TransportError>) -> Wire {
        Wire { otherwise, ..self }
    }

    /// Every request, in order.
    pub(crate) fn sent(&self) -> Vec<Request> {
        self.sent.borrow().clone()
    }

    /// Every request as its URL and its body's text.
    pub(crate) fn bodies(&self) -> Vec<(String, String)> {
        self.sent()
            .into_iter()
            .map(|request| (request.url, String::from_utf8_lossy(&request.body).into_owned()))
            .collect()
    }
}

impl Http for Wire {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>> {
        self.sent.borrow_mut().push(request);
        ready(self.answers.borrow_mut().pop_front().unwrap_or_else(|| self.otherwise.clone()))
    }
}

/// A `200` with `text` as its body.
pub(crate) fn ok(text: &str) -> Result<Response, TransportError> {
    status(200, text)
}

/// An answer with `status` and `text` as its body.
pub(crate) fn status(status: u16, text: &str) -> Result<Response, TransportError> {
    Ok(Response { status, headers: Vec::new(), body: text.as_bytes().to_vec() })
}

/// `vectors/lighter_go.json`: what lighter-go and go-ethereum sign, with every random draw they
/// took.
pub(crate) fn vectors() -> Value {
    serde_json::from_str(include_str!("../vectors/lighter_go.json")).unwrap_or(Value::Null)
}

#[cfg(feature = "sign")]
/// The case named `name`, among the transactions, the tokens or the EVM cases.
pub(crate) fn vector(name: &str) -> Value {
    let vectors = vectors();
    ["cases", "tokens", "evm"]
        .iter()
        .filter_map(|list| vectors[list].as_array())
        .flatten()
        .find(|case| case["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no vector named {name}"))
}

/// The private key a case's signer signs with.
pub(crate) fn vector_key(case: &Value) -> SecretKey {
    let name = case["signer"]["key"].as_str().unwrap_or_default();
    let hex = vectors()["keys"][name]["private_key"].as_str().unwrap_or_default().to_owned();
    SecretKey::from_hex(&hex).unwrap_or_else(|error| panic!("key {name}: {error}"))
}

#[cfg(feature = "sign")]
/// The wallet's private key: Hardhat's first account.
pub(crate) fn vector_wallet_key() -> String {
    vectors()["wallet"]["private_key"].as_str().unwrap_or_default().to_owned()
}

/// A case's chain id and envelope.
pub(crate) fn vector_envelope(case: &Value) -> (u32, Envelope) {
    let signer = &case["signer"];
    let number = |value: &Value| value.as_u64().unwrap_or_else(|| panic!("not a number: {value}"));
    let chain_id = u32::try_from(number(&signer["chain_id"])).unwrap_or_default();
    let account = AccountIndex::new(number(&signer["account_index"])).unwrap_or_else(|error| panic!("{error}"));
    let slot = u8::try_from(number(&signer["api_key_index"])).unwrap_or_default();
    let api_key = ApiKeyIndex::new(slot).unwrap_or_else(|error| panic!("{error}"));
    let nonce = case["nonce"].as_u64().unwrap_or_default();
    let expired_at = case["expired_at"].as_u64().unwrap_or_default();
    (chain_id, Envelope { account, api_key, nonce, expired_at })
}

/// Hands out a case's recorded draws, in order, as lighter-go read them.
pub(crate) fn replay(case: &Value) -> impl FnMut(&mut [u8; 40]) -> Result<(), String> + use<> {
    let draws: VecDeque<Vec<u8>> = case["draws"]
        .as_array()
        .map(|draws| {
            draws.iter().filter_map(|draw| crate::utils::hex::decode(draw.as_str().unwrap_or_default()).ok()).collect()
        })
        .unwrap_or_default();
    let draws = RefCell::new(draws);
    move |buffer: &mut [u8; 40]| {
        let next = draws.borrow_mut().pop_front().ok_or_else(|| "no draws left".to_owned())?;
        buffer.copy_from_slice(&next);
        Ok(())
    }
}
