//! [`Rpc`], one chain's JSON-RPC: a URL and the wire to it, each method sent once and its answer
//! read; and the steps every transaction signed here takes: its nonce, gas and fees asked of the
//! node, signed, sent once, and its receipt waited for.

#[cfg(feature = "sign")]
use crate::account::keys::LocalWallet;
use crate::client::evm::records::{FeeHistory, Receipt};
use crate::client::evm::rpc::{BlockTag, CallRequest, L1Error, Query, RpcError, eth};
use crate::client::evm::tx::{Call, Eip1559, Fees, with_headroom};
use crate::client::evm::u256::{B256, U256};
use crate::client::{Http, Network, Request, TransportError};
use crate::utils::Address;

/// How long [`Rpc::wait_for_receipt`] pauses between two reads: about a quarter of an Ethereum
/// block, many of Robinhood Chain's.
pub const RECEIPT_POLL_MS: u64 = 3_000;

/// How many reads [`Rpc::wait_for_receipt`] makes before it gives up: ten minutes at
/// [`RECEIPT_POLL_MS`].
pub const RECEIPT_TRIES: u32 = 200;

/// One chain's JSON-RPC: a URL and the wire to it.
#[derive(Debug, Clone)]
pub struct Rpc<H> {
    http: H,
    url: String,
}

impl<H: Http> Rpc<H> {
    /// Any node.
    pub fn new(http: H, url: impl Into<String>) -> Self {
        Rpc { http, url: url.into() }
    }

    /// The public node on `network`'s L1 ([`Network::l1_rpc_url`]): Ethereum, or Robinhood
    /// Chain. A program in production brings its own.
    pub fn l1(http: H, network: Network) -> Self {
        Rpc::new(http, network.l1_rpc_url())
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub const fn http(&self) -> &H {
        &self.http
    }

    /// Posts the query once and reads the answer. Nothing is retried.
    pub async fn run<T>(&self, query: Query<T>) -> Result<T, RpcError> {
        let response = self.http.send(Request::post_json(self.url.clone(), query.body())).await?;
        if response.status == 429 {
            return Err(TransportError::RateLimited { retry_after_ms: response.retry_after_ms() }.into());
        }
        let text = response.text();
        match (response.is_success(), query.answer(&text)) {
            (_, Err(refused @ RpcError::Node { .. })) => Err(refused),
            (true, answer) => answer,
            (false, _) => Err(TransportError::Status { status: response.status, body: text }.into()),
        }
    }

    pub async fn chain_id(&self) -> Result<u64, RpcError> {
        self.run(eth::chain_id()).await
    }

    pub async fn balance(&self, owner: Address, block: BlockTag) -> Result<U256, RpcError> {
        self.run(eth::balance(owner, block)).await
    }

    pub async fn transaction_count(&self, owner: Address, block: BlockTag) -> Result<u64, RpcError> {
        self.run(eth::transaction_count(owner, block)).await
    }

    pub async fn gas_price(&self) -> Result<U256, RpcError> {
        self.run(eth::gas_price()).await
    }

    pub async fn max_priority_fee(&self) -> Result<U256, RpcError> {
        self.run(eth::max_priority_fee()).await
    }

    pub async fn fee_history(&self, blocks: u64, newest: BlockTag, percentiles: &[u8]) -> Result<FeeHistory, RpcError> {
        self.run(eth::fee_history(blocks, newest, percentiles)).await
    }

    pub async fn estimate_gas(&self, call: &CallRequest) -> Result<u64, RpcError> {
        self.run(eth::estimate_gas(call)).await
    }

    pub async fn call(&self, call: &CallRequest, block: BlockTag) -> Result<Vec<u8>, RpcError> {
        self.run(eth::call(call, block)).await
    }

    /// Sent once. With no answer the transaction may still have landed
    /// ([`RpcError::may_have_landed`]): look for its receipt by
    /// [`crate::client::evm::tx::tx_hash`] before sending it again.
    pub async fn send_raw_transaction(&self, raw: &[u8]) -> Result<B256, RpcError> {
        self.run(eth::send_raw_transaction(raw)).await
    }

    pub async fn receipt(&self, hash: B256) -> Result<Option<Receipt>, RpcError> {
        self.run(eth::receipt(hash)).await
    }

    /// Refused unless the node is on `chain_id`: a deposit signed for one chain is only ever sent
    /// to that chain.
    pub async fn check_chain(&self, chain_id: u64) -> Result<(), L1Error> {
        match self.chain_id().await? {
            found if found == chain_id => Ok(()),
            found => Err(L1Error::WrongChain { expected: chain_id, found }),
        }
    }

    /// The fees to offer now: the node's suggested tip, and a cap of twice the next block's base
    /// fee plus that tip ([`Fees::over_base`]).
    pub async fn fees(&self) -> Result<Fees, RpcError> {
        let history = self.fee_history(1, BlockTag::Latest, &[]).await?;
        let base = history.next_base_fee().ok_or_else(|| RpcError::Unreadable {
            method: "eth_feeHistory",
            detail: "no base fee for the next block".to_owned(),
        })?;
        let tip = self.max_priority_fee().await?;
        Ok(Fees::over_base(base, tip))
    }

    /// The transaction `call` makes from `from` on `chain_id`: the next nonce counting the pool,
    /// the node's gas estimate with headroom, and [`Rpc::fees`]. The estimate runs the call, so
    /// a call that would revert now (a deposit before its approval is mined) is refused here.
    pub async fn prepare(&self, chain_id: u64, from: Address, call: Call) -> Result<Eip1559, RpcError> {
        let nonce = self.transaction_count(from, BlockTag::Pending).await?;
        let fees = self.fees().await?;
        let gas = self.estimate_gas(&CallRequest::of(Some(from), &call)).await?;
        Ok(call.transaction(chain_id, nonce, with_headroom(gas), fees))
    }

    /// [`Rpc::prepare`]d from `wallet`'s address and signed by it: the raw bytes to send. Nothing
    /// is sent.
    #[cfg(feature = "sign")]
    pub async fn sign_call(&self, wallet: &LocalWallet, chain_id: u64, call: Call) -> Result<Vec<u8>, L1Error> {
        let tx = self.prepare(chain_id, wallet.address(), call).await?;
        Ok(tx.sign(wallet)?)
    }

    /// Signs `call` ([`Rpc::sign_call`]) and sends it once: its hash.
    #[cfg(feature = "sign")]
    pub async fn send_call(&self, wallet: &LocalWallet, chain_id: u64, call: Call) -> Result<B256, L1Error> {
        let raw = self.sign_call(wallet, chain_id, call).await?;
        Ok(self.send_raw_transaction(&raw).await?)
    }

    /// The receipt of `hash` once it is mined, read every [`RECEIPT_POLL_MS`] up to
    /// [`RECEIPT_TRIES`] times; `None` when it never came. `pause` waits the milliseconds it is
    /// given: `tokio::time::sleep` natively, a timer in the browser. A read that fails ends the
    /// wait with its error; the transaction may still land.
    pub async fn wait_for_receipt(&self, hash: B256, pause: impl AsyncFn(u64)) -> Result<Option<Receipt>, RpcError> {
        for attempt in 0..RECEIPT_TRIES {
            if attempt > 0 {
                pause(RECEIPT_POLL_MS).await;
            }
            if let Some(receipt) = self.receipt(hash).await? {
                return Ok(Some(receipt));
            }
        }
        Ok(None)
    }
}
