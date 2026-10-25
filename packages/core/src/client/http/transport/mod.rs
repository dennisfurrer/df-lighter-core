//! The [`Http`] wire and the [`Transport`] trait: any request out with its method, headers and
//! body, the GETs and form POSTs under `/api/v1` every read and send goes through, and what
//! `sendTx` answers.

pub mod answer;
pub mod calls;
pub mod wire;

pub use answer::{MOST_IN_BATCH, Sent, SentBatch, send_tx, send_tx_batch, send_tx_batch_call, send_tx_call};
pub use calls::{API_PREFIX, ApiError, Call, Transport, codes, decode, read};
pub use wire::{Http, Method, OFFLINE, Request, Response, TransportError, form};
