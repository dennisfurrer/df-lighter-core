//! The recorded frames, and what the live tests share to read them. Every frame is the text as
//! received, one a line.
//!
//! - `fixtures/lighter/stream.jsonl`: Lighter's socket on 2026-10-25, public channels only, the
//!   first frames of each: BTC's book, ticker, trades and candles, ETH/USDC's figures, account
//!   10's public channels, every binary option's figures, the BTC 15 minute Up/Down series, the
//!   height, a pong and a refused channel.
//! - `fixtures/robinhood/stream.jsonl`: Robinhood Chain's the same minute: BTC's book and trades,
//!   every perp's and spot market's figures, the height, a pong and a refused channel.
//! - `fixtures/lighter/stream_answers.jsonl`: Lighter's read-only socket answering what a client
//!   sends, the same morning: the greeting, a second subscribe, an unsubscribe and its repeat, a
//!   private channel without its token, one binary option's figures.
//!
//! Private channels and transaction answers were not recorded (no key signs here); their frames
//! in the tests are the docs' examples, or the shape lighter-ts-sdk 1.1.0 notes as verified live.

use crate::client::Network;

const LIGHTER: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/lighter/stream.jsonl"));
const ROBINHOOD: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/robinhood/stream.jsonl"));
const ANSWERS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/lighter/stream_answers.jsonl"));

/// Every frame one venue's recording holds, in the order received.
pub fn every(network: Network) -> Vec<&'static str> {
    let text = match network {
        Network::Lighter => LIGHTER,
        Network::Robinhood => ROBINHOOD,
    };
    text.lines().filter(|line| !line.trim().is_empty()).collect()
}

/// The frames whose `type` is `kind` and whose channel is `channel` (any, when empty), in order.
pub fn all(network: Network, kind: &str, channel: &str) -> Vec<&'static str> {
    every(network)
        .into_iter()
        .filter(|line| line.contains(&format!(r#""type":"{kind}""#)))
        .filter(|line| channel.is_empty() || line.contains(&format!(r#""channel":"{channel}""#)))
        .collect()
}

/// The first frame whose `type` is `kind` and whose channel is `channel` (any, when empty).
pub fn first(network: Network, kind: &str, channel: &str) -> &'static str {
    all(network, kind, channel).into_iter().next().unwrap_or_else(|| panic!("no {kind} {channel} on {network}"))
}

/// The first Lighter frame of `kind` on `channel`.
pub fn lighter(kind: &str, channel: &str) -> &'static str {
    first(Network::Lighter, kind, channel)
}

/// The first Robinhood Chain frame of `kind` on `channel`.
pub fn robinhood(kind: &str, channel: &str) -> &'static str {
    first(Network::Robinhood, kind, channel)
}

/// The read-only socket's answers, in order.
pub fn answers() -> Vec<&'static str> {
    ANSWERS.lines().filter(|line| !line.trim().is_empty()).collect()
}

/// The answer that contains `text`.
pub fn answer(text: &str) -> &'static str {
    answers().into_iter().find(|line| line.contains(text)).unwrap_or_else(|| panic!("no answer with {text}"))
}

/// The venue's refusal of a channel it does not serve, recorded on both.
pub const INVALID_CHANNEL: &str = r#"{"error":{"code":30005,"message":"Invalid Channel"}}"#;

/// A resting order as the account channels write one (the docs' Order JSON).
pub const ORDER: &str = r#"{
    "order_index": 281476612587355, "client_order_index": 42, "order_id": "281476612587355",
    "client_order_id": "42", "market_index": 1, "market_symbol": "BTC", "market_kind": "perps",
    "owner_account_index": 10, "initial_base_amount": "0.50000", "price": "90000.0", "nonce": 7,
    "remaining_base_amount": "0.20000", "is_ask": true, "base_size": 50000, "base_price": 900000,
    "filled_base_amount": "0.30000", "filled_quote_amount": "27000.000000", "side": "sell",
    "type": "limit", "time_in_force": "good-till-time", "reduce_only": false,
    "trigger_price": "0", "order_expiry": 1792918063399, "status": "open", "trigger_status": "na",
    "trigger_time": 0, "parent_order_index": 0, "parent_order_id": "", "to_trigger_order_id_0": "",
    "to_trigger_order_id_1": "", "to_cancel_order_id_0": "", "integrator_fee_collector_index": "",
    "integrator_taker_fee": "", "integrator_maker_fee": "", "order_flags": 0, "block_height": 198321831,
    "timestamp": 1773854156, "created_at": 1773854156654, "updated_at": 1773854156700,
    "transaction_time": 1773854156686065, "order_version": 0
}"#;

/// The docs' example position.
pub const POSITION: &str = r#"{
    "market_id": 1, "symbol": "BTC", "initial_margin_fraction": "0.1", "open_order_count": 2,
    "pending_order_count": 1, "position_tied_order_count": 3, "sign": 1, "position": "0.5",
    "avg_entry_price": "20000.00", "position_value": "10000.00", "unrealized_pnl": "500.00",
    "realized_pnl": "100.00", "liquidation_price": "3024.66", "total_funding_paid_out": "34.2",
    "margin_mode": 1, "allocated_margin": "46342"
}"#;

/// The docs' example trade.
pub const TRADE: &str = r#"{
    "trade_id":16164557907, "trade_id_str":"16164557907",
    "tx_hash":"019f2b9c9cc609196316a569541e135a739728e0837fcfb05c913534e305e503c01d5dcfeabeaf81",
    "type":"trade", "market_id":0, "market_symbol":"ETH", "market_kind":"perps", "size":"0.1336",
    "price":"2181.83", "usd_amount":"291.492488", "ask_id":281476612587355, "ask_id_str":"281476612587355",
    "bid_id":562948334068259, "bid_id_str":"562948334068259", "ask_client_id":363283,
    "ask_client_id_str":"363283", "bid_client_id":23004521241, "bid_client_id_str":"23004521241",
    "ask_account_id":57890, "bid_account_id":10, "is_maker_ask":false, "block_height":198321831,
    "timestamp":1773854156654, "taker_fee": 196, "maker_fee":28, "transaction_time":1773854156686065
}"#;

/// The docs' example funding payment.
pub const FUNDING: &str = r#"{"timestamp": 1773850000, "market_id": 0, "funding_id": 2001, "change": "0.001",
    "rate": "0.0001", "position_size": "0.5", "position_side": "long", "discount": "0"}"#;
