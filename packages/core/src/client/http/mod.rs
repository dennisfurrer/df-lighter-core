//! HTTP to the REST API: the [`Http`] wire, the [`Transport`] every read and send goes through,
//! what `sendTx` answers, and the rate limit.

mod api;
pub mod limit;
pub mod transport;

pub(crate) use api::Api;
pub use limit::{Kind, Spent, Tier};
pub use transport::{
    API_PREFIX, ApiError, Call, Http, MOST_IN_BATCH, Method, OFFLINE, Request, Response, Sent, SentBatch, Transport,
    TransportError, codes, decode, form, read, send_tx, send_tx_batch, send_tx_batch_call, send_tx_call,
};
