//! Reading one text frame ([`decode`]): its `type` says what it is, and for a channel's frame
//! the channel it names (`order_book:1`, `candle:1:1m`) says the market or account the data is
//! about where the data does not. A frame that does not read is a [`BadFrame`], naming the channel
//! when it got that far.

use std::{borrow::Cow, fmt, str::FromStr};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::value::RawValue;

use crate::account::history::{PositionFunding, Transaction};
use crate::account::holdings::BoPosition;
use crate::account::{Asset, Order, PoolShare, Position};
use crate::client::{Sent, SentBatch};
use crate::earn::pools::PoolInfo;
use crate::live::account::{
    AccountAll, AccountAllAssets, AccountAllOrders, AccountAllPositions, AccountAllTrades, AccountMarket,
    AccountOrders, AccountTx, AvgEntryPrice, AvgEntryPrices, LivePoints, Notification, Notifications, PoolData, Rfq,
    Stats, UserStats, Volumes,
};
use crate::live::frame::fields::{assets, id_text, one, rows};
use crate::live::market::book::side;
use crate::live::market::{
    BoMarketStats, BookUpdate, Candle, Candles, Height, Level, MarkCandle, MarkPriceCandles, MarketStats, Resolution,
    SpotMarketStats, Ticker, Trades, UpDownCandle, UpDownCandles, UpDownResolution,
};
use crate::live::{Data, Frame, Refusal};
use crate::markets::Trade;
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, Dec, Figure, MarketId};

/// A frame that did not read: the channel it named (or its `type`), when it got that far, and
/// why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadFrame {
    pub channel: Option<String>,
    pub detail: String,
}

impl fmt::Display for BadFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.channel {
            Some(channel) => write!(f, "a `{channel}` frame did not read: {}", self.detail),
            None => write!(f, "not a frame: {}", self.detail),
        }
    }
}

impl std::error::Error for BadFrame {}

/// What every frame may carry at its top level.
#[derive(Deserialize)]
struct Head<'a> {
    #[serde(default, rename = "type", borrow)]
    kind: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    channel: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    error: Option<&'a RawValue>,
    #[serde(default, deserialize_with = "id_text")]
    id: Option<String>,
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default, borrow)]
    tx_hash: Option<&'a RawValue>,
    #[serde(default)]
    session_id: Option<String>,
}

/// `{"code":..,"message":..}`, or a bare message.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawError {
    Coded {
        #[serde(default)]
        code: i64,
        #[serde(default)]
        message: String,
    },
    Said(String),
}

/// Reads one text frame.
pub fn decode(text: &str) -> Result<Frame, BadFrame> {
    let head: Head<'_> = serde_json::from_str(text)
        .map_err(|error| BadFrame { channel: None, detail: format!("{error} in {}", shown(text)) })?;
    let kind = head.kind.as_deref().unwrap_or_default();
    let channel = head.channel.as_deref().unwrap_or_default();
    if let Some(error) = head.error {
        let refused: RawError = serde_json::from_str(error.get())
            .map_err(|error| BadFrame { channel: Some("error".to_owned()), detail: error.to_string() })?;
        let (code, message) = match refused {
            RawError::Coded { code, message } => (code, message),
            RawError::Said(message) => (0, message),
        };
        return Ok(Frame::Error(Refusal { code, message, id: head.id }));
    }
    if let Some(name) = kind.strip_prefix("subscribed/") {
        return Ok(Frame::Snapshot(read(name, channel, text)?));
    }
    if let Some(name) = kind.strip_prefix("update/") {
        return Ok(Frame::Update(read(name, channel, text)?));
    }
    match kind {
        "connected" => return Ok(Frame::Connected { session_id: head.session_id.unwrap_or_default() }),
        "pong" => return Ok(Frame::Pong),
        "ping" => return Ok(Frame::Ping),
        "unsubscribed" => return Ok(Frame::Unsubscribed { channel: channel.to_owned() }),
        _ => {}
    }
    let refused = head.code.filter(|code| !matches!(code, 0 | 200));
    if let Some(code) = refused {
        return Ok(Frame::Error(Refusal { code, message: head.message.unwrap_or_default(), id: head.id }));
    }
    if let Some(hash) = head.tx_hash {
        let id = head.id.unwrap_or_default();
        let fail = |error: serde_json::Error| BadFrame {
            channel: Some("jsonapi/sendtx".to_owned()),
            detail: error.to_string(),
        };
        return Ok(match hash.get().trim_start().starts_with('[') {
            true => Frame::SentBatch { id, sent: serde_json::from_str::<SentBatch>(text).map_err(fail)? },
            false => Frame::Sent { id, sent: serde_json::from_str::<Sent>(text).map_err(fail)? },
        });
    }
    match kind {
        "" => Err(BadFrame { channel: None, detail: format!("no type in {}", shown(text)) }),
        other => Ok(Frame::Other { kind: other.to_owned(), text: text.to_owned() }),
    }
}

/// A channel's frame, read for the channel `name` its type names.
fn read(name: &str, channel: &str, text: &str) -> Result<Data, BadFrame> {
    let typed = Typed { name, channel, parts: channel.split(':').skip(1).collect(), text };
    Ok(match name {
        "order_book" => Data::OrderBook(typed.book()?),
        "ticker" => Data::Ticker(typed.ticker()?),
        "market_stats" => Data::MarketStats(typed.read::<PerpStats>()?.market_stats),
        "spot_market_stats" => Data::SpotMarketStats(typed.read::<SpotStats>()?.spot_market_stats),
        "bo_market_stats" => Data::BoMarketStats(typed.read::<OptionStats>()?.bo_market_stats),
        "trade" => {
            let raw: RawTrades = typed.read()?;
            Data::Trades(Trades {
                market: typed.part(0, "market")?,
                trades: raw.trades,
                liquidations: raw.liquidation_trades,
                nonce: raw.nonce,
            })
        }
        "candle" => {
            let raw: RawCandles<Candle> = typed.read()?;
            Data::Candles(Candles {
                market: typed.part(0, "market")?,
                resolution: typed.resolution(1)?,
                candles: raw.candles,
                timestamp: raw.timestamp,
            })
        }
        "mark_price_candle" => {
            let raw: RawCandles<MarkCandle> = typed.read()?;
            Data::MarkPriceCandles(MarkPriceCandles {
                market: typed.part(0, "market")?,
                resolution: typed.resolution(1)?,
                candles: raw.candles,
                timestamp: raw.timestamp,
            })
        }
        "up_down_candle" => {
            let raw: RawCandles<UpDownCandle> = typed.read()?;
            let resolution = typed.word(2, "resolution")?;
            Data::UpDownCandles(UpDownCandles {
                symbol: typed.word(0, "symbol")?.to_owned(),
                duration: typed.word(1, "duration")?.to_owned(),
                resolution: UpDownResolution::parse(resolution)
                    .ok_or_else(|| typed.fail(format!("`{resolution}` is not a resolution")))?,
                candles: raw.candles,
                timestamp: raw.timestamp,
            })
        }
        "height" => {
            let raw: RawHeight = typed.read()?;
            Data::Height(Height { height: raw.height, timestamp: raw.timestamp })
        }
        "account_all" => Data::AccountAll(Box::new(typed.account_all()?)),
        "account_market" => Data::AccountMarket(Box::new(typed.account_market()?)),
        "user_stats" => Data::UserStats(Box::new(typed.user_stats()?)),
        "account_tx" => {
            let raw: RawTxs = typed.read()?;
            Data::AccountTx(AccountTx { account: typed.part(0, "account")?, txs: raw.txs })
        }
        "account_all_orders" => {
            let raw: RawOrders = typed.read()?;
            Data::AccountAllOrders(AccountAllOrders { account: typed.part(0, "account")?, orders: raw.orders })
        }
        "account_orders" => {
            let raw: RawOrders = typed.read()?;
            let account = match raw.account {
                Some(account) => account,
                None => typed.part(1, "account")?,
            };
            Data::AccountOrders(AccountOrders {
                market: typed.part(0, "market")?,
                account,
                orders: raw.orders,
                nonce: raw.nonce,
            })
        }
        "account_all_trades" => {
            let raw: RawAccountTrades = typed.read()?;
            Data::AccountAllTrades(AccountAllTrades {
                account: typed.part(0, "account")?,
                trades: raw.trades,
                volumes: volumes(
                    [&raw.daily_volume, &raw.weekly_volume, &raw.monthly_volume, &raw.total_volume],
                    [raw.daily_trades_count, raw.weekly_trades_count, raw.monthly_trades_count, raw.total_trades_count],
                ),
            })
        }
        "account_all_positions" => {
            let raw: RawPositions = typed.read()?;
            Data::AccountAllPositions(AccountAllPositions {
                account: typed.part(0, "account")?,
                positions: raw.positions,
                bo_positions: raw.bo_positions,
                shares: raw.shares,
                last_funding_round: raw.last_funding_round.unwrap_or_default().into_iter().collect(),
                last_funding_discount: raw.last_funding_discount.unwrap_or_default().into_iter().collect(),
                transaction_time: raw.transaction_time,
            })
        }
        "account_all_assets" => {
            let raw: RawAssets = typed.read()?;
            Data::AccountAllAssets(AccountAllAssets {
                account: typed.part(0, "account")?,
                assets: raw.assets,
                timestamp: raw.timestamp,
                transaction_time: raw.transaction_time,
            })
        }
        "account_spot_avg_entry_prices" => {
            let raw: RawAvgEntryPrices = typed.read()?;
            Data::AvgEntryPrices(AvgEntryPrices {
                account: typed.part(0, "account")?,
                prices: raw.avg_entry_prices,
                timestamp: raw.timestamp,
            })
        }
        "pool_data" => {
            let raw: RawPoolData = typed.read()?;
            let account = match raw.account {
                Some(account) => account,
                None => typed.part(0, "account")?,
            };
            Data::PoolData(Box::new(PoolData {
                account,
                trades: raw.trades,
                orders: raw.orders,
                positions: raw.positions,
                shares: raw.shares,
                fundings: raw.funding_histories,
            }))
        }
        "pool_info" => {
            let raw: RawPoolInfo = typed.read()?;
            Data::PoolInfo { account: typed.part(0, "account")?, info: Box::new(raw.pool_info) }
        }
        "notification" => {
            let raw: RawNotes = typed.read()?;
            Data::Notifications(Notifications { account: typed.part(0, "account")?, notes: raw.notifs })
        }
        "rfq" => Data::Rfqs(typed.read::<RawRfqs>()?.rfqs),
        "live_points" => {
            let raw: RawPoints = typed.read()?;
            Data::LivePoints(LivePoints { account: typed.part(0, "account")?, total: raw.total_live_points })
        }
        _ => Data::Other { channel: channel.to_owned(), text: text.to_owned() },
    })
}

/// A frame's text, read for the channel that named it.
struct Typed<'a> {
    name: &'a str,
    channel: &'a str,
    /// The channel's parts after its name.
    parts: Vec<&'a str>,
    text: &'a str,
}

impl Typed<'_> {
    fn fail(&self, detail: String) -> BadFrame {
        let channel = if self.channel.is_empty() { self.name } else { self.channel };
        BadFrame { channel: Some(channel.to_owned()), detail }
    }

    fn read<T: DeserializeOwned>(&self) -> Result<T, BadFrame> {
        serde_json::from_str(self.text).map_err(|error| self.fail(format!("{error} in {}", shown(self.text))))
    }

    fn word(&self, at: usize, what: &str) -> Result<&str, BadFrame> {
        self.parts
            .get(at)
            .copied()
            .filter(|part| !part.is_empty())
            .ok_or_else(|| self.fail(format!("the channel names no {what}")))
    }

    fn part<T: FromStr>(&self, at: usize, what: &str) -> Result<T, BadFrame> {
        let word = self.word(at, what)?;
        word.parse().map_err(|_| self.fail(format!("`{word}` is not a {what}")))
    }

    fn resolution(&self, at: usize) -> Result<Resolution, BadFrame> {
        let word = self.word(at, "resolution")?;
        Resolution::parse(word).ok_or_else(|| self.fail(format!("`{word}` is not a resolution")))
    }

    fn book(&self) -> Result<BookUpdate, BadFrame> {
        let raw: RawBook = self.read()?;
        Ok(BookUpdate {
            market: self.part(0, "market")?,
            asks: raw.order_book.asks.unwrap_or_default(),
            bids: raw.order_book.bids.unwrap_or_default(),
            nonce: raw.order_book.nonce,
            begin_nonce: raw.order_book.begin_nonce,
            offset: raw.order_book.offset.or(raw.offset).unwrap_or_default(),
            last_updated_at: raw.order_book.last_updated_at.or(raw.last_updated_at).unwrap_or_default(),
            timestamp: raw.timestamp,
        })
    }

    fn ticker(&self) -> Result<Ticker, BadFrame> {
        let raw: RawTicker = self.read()?;
        Ok(Ticker {
            market: self.part(0, "market")?,
            symbol: raw.ticker.s,
            ask: raw.ticker.a,
            bid: raw.ticker.b,
            nonce: raw.nonce,
            last_updated_at: raw.ticker.last_updated_at.or(raw.last_updated_at).unwrap_or_default(),
            timestamp: raw.timestamp,
        })
    }

    fn account_all(&self) -> Result<AccountAll, BadFrame> {
        let raw: RawAccountAll = self.read()?;
        let account = match raw.account {
            Some(account) => account,
            None => self.part(0, "account")?,
        };
        Ok(AccountAll {
            account,
            assets: raw.assets,
            positions: raw.positions,
            bo_positions: raw.bo_positions,
            shares: raw.shares,
            trades: raw.trades,
            fundings: raw.funding_histories,
            volumes: volumes(
                [&raw.daily_volume, &raw.weekly_volume, &raw.monthly_volume, &raw.total_volume],
                [raw.daily_trades_count, raw.weekly_trades_count, raw.monthly_trades_count, raw.total_trades_count],
            ),
            transaction_time: raw.transaction_time,
        })
    }

    fn account_market(&self) -> Result<AccountMarket, BadFrame> {
        let raw: RawAccountMarket = self.read()?;
        let account = match raw.account {
            Some(account) => account,
            None => self.part(1, "account")?,
        };
        Ok(AccountMarket {
            market: self.part(0, "market")?,
            account,
            assets: raw.assets,
            fundings: raw.funding_history,
            orders: raw.orders,
            position: raw.position,
            bo_position: raw.bo_position,
            trades: raw.trades,
            transaction_time: raw.transaction_time,
        })
    }

    fn user_stats(&self) -> Result<UserStats, BadFrame> {
        let raw: RawUserStats = self.read()?;
        Ok(UserStats {
            account: self.part(0, "account")?,
            stats: Stats {
                collateral: raw.stats.collateral,
                portfolio_value: raw.stats.portfolio_value,
                leverage: raw.stats.leverage,
                available_balance: raw.stats.available_balance,
                margin_usage: raw.stats.margin_usage,
                buying_power: raw.stats.buying_power,
            },
            trading_mode: raw.stats.account_trading_mode,
            cross: raw.stats.cross_stats,
            total: raw.stats.total_stats,
            timestamp: raw.timestamp,
        })
    }
}

#[derive(Deserialize)]
struct RawBook {
    #[serde(default)]
    offset: Option<u64>,
    #[serde(default)]
    last_updated_at: Option<u64>,
    order_book: RawBookSides,
    #[serde(default, deserialize_with = "whole")]
    timestamp: u64,
}

#[derive(Deserialize)]
struct RawBookSides {
    #[serde(default)]
    asks: Option<Vec<Level>>,
    #[serde(default)]
    bids: Option<Vec<Level>>,
    #[serde(deserialize_with = "whole")]
    nonce: u64,
    #[serde(default, deserialize_with = "whole")]
    begin_nonce: u64,
    #[serde(default)]
    offset: Option<u64>,
    #[serde(default)]
    last_updated_at: Option<u64>,
}

#[derive(Deserialize)]
struct RawTicker {
    #[serde(default, deserialize_with = "whole")]
    nonce: u64,
    #[serde(default)]
    last_updated_at: Option<u64>,
    ticker: RawBbo,
    #[serde(default, deserialize_with = "whole")]
    timestamp: u64,
}

#[derive(Deserialize)]
struct RawBbo {
    #[serde(default)]
    s: String,
    #[serde(default, deserialize_with = "side")]
    a: Option<Level>,
    #[serde(default, deserialize_with = "side")]
    b: Option<Level>,
    #[serde(default)]
    last_updated_at: Option<u64>,
}

#[derive(Deserialize)]
struct PerpStats {
    #[serde(deserialize_with = "rows")]
    market_stats: Vec<MarketStats>,
}

#[derive(Deserialize)]
struct SpotStats {
    #[serde(deserialize_with = "rows")]
    spot_market_stats: Vec<SpotMarketStats>,
}

#[derive(Deserialize)]
struct OptionStats {
    #[serde(deserialize_with = "rows")]
    bo_market_stats: Vec<BoMarketStats>,
}

#[derive(Deserialize)]
struct RawTrades {
    #[serde(default, deserialize_with = "rows")]
    trades: Vec<Trade>,
    #[serde(default, deserialize_with = "rows")]
    liquidation_trades: Vec<Trade>,
    #[serde(default, deserialize_with = "whole")]
    nonce: u64,
}

#[derive(Deserialize)]
#[serde(bound = "T: DeserializeOwned")]
struct RawCandles<T> {
    #[serde(default, deserialize_with = "rows")]
    candles: Vec<T>,
    #[serde(default, deserialize_with = "whole")]
    timestamp: u64,
}

#[derive(Deserialize)]
struct RawHeight {
    #[serde(deserialize_with = "whole")]
    height: u64,
    #[serde(default, deserialize_with = "whole")]
    timestamp: u64,
}

/// The account's trade volumes, where a frame says them: all four, or nothing (an update says
/// none of them). Read as plain fields, never a flattened struct: a [`Figure`] reads only from the
/// text as written.
fn volumes(figures: [&Option<Figure>; 4], counts: [Option<u64>; 4]) -> Option<Volumes> {
    let [daily, weekly, monthly, total] = figures;
    let [daily_trades, weekly_trades, monthly_trades, total_trades] = counts;
    Some(Volumes {
        daily: daily.clone()?,
        weekly: weekly.clone()?,
        monthly: monthly.clone()?,
        total: total.clone()?,
        daily_trades,
        weekly_trades,
        monthly_trades,
        total_trades,
    })
}

#[derive(Deserialize)]
struct RawAccountAll {
    #[serde(default)]
    account: Option<AccountIndex>,
    #[serde(default, deserialize_with = "assets")]
    assets: Vec<Asset>,
    #[serde(default, deserialize_with = "rows")]
    positions: Vec<Position>,
    #[serde(default, deserialize_with = "rows")]
    bo_positions: Vec<BoPosition>,
    #[serde(default, deserialize_with = "rows")]
    shares: Vec<PoolShare>,
    #[serde(default, deserialize_with = "rows")]
    trades: Vec<Trade>,
    #[serde(default, deserialize_with = "rows")]
    funding_histories: Vec<PositionFunding>,
    #[serde(default)]
    daily_volume: Option<Figure>,
    #[serde(default)]
    weekly_volume: Option<Figure>,
    #[serde(default)]
    monthly_volume: Option<Figure>,
    #[serde(default)]
    total_volume: Option<Figure>,
    #[serde(default)]
    daily_trades_count: Option<u64>,
    #[serde(default)]
    weekly_trades_count: Option<u64>,
    #[serde(default)]
    monthly_trades_count: Option<u64>,
    #[serde(default)]
    total_trades_count: Option<u64>,
    #[serde(default, deserialize_with = "whole")]
    transaction_time: u64,
}

#[derive(Deserialize)]
struct RawAccountMarket {
    #[serde(default)]
    account: Option<AccountIndex>,
    #[serde(default, deserialize_with = "assets")]
    assets: Vec<Asset>,
    #[serde(default, deserialize_with = "rows")]
    funding_history: Vec<PositionFunding>,
    #[serde(default, deserialize_with = "rows")]
    orders: Vec<Order>,
    #[serde(default, deserialize_with = "one")]
    position: Option<Position>,
    #[serde(default, deserialize_with = "one")]
    bo_position: Option<BoPosition>,
    #[serde(default, deserialize_with = "rows")]
    trades: Vec<Trade>,
    #[serde(default)]
    transaction_time: Option<u64>,
}

#[derive(Deserialize)]
struct RawUserStatsInner {
    collateral: Dec,
    portfolio_value: Dec,
    leverage: Dec,
    available_balance: Dec,
    margin_usage: Dec,
    buying_power: Dec,
    #[serde(default)]
    account_trading_mode: u8,
    #[serde(default)]
    cross_stats: Option<Stats>,
    #[serde(default)]
    total_stats: Option<Stats>,
}

#[derive(Deserialize)]
struct RawUserStats {
    stats: RawUserStatsInner,
    #[serde(default, deserialize_with = "whole")]
    timestamp: u64,
}

#[derive(Deserialize)]
struct RawTxs {
    #[serde(default, deserialize_with = "rows")]
    txs: Vec<Transaction>,
}

#[derive(Deserialize)]
struct RawOrders {
    #[serde(default)]
    account: Option<AccountIndex>,
    #[serde(default, deserialize_with = "rows")]
    orders: Vec<Order>,
    #[serde(default, deserialize_with = "whole")]
    nonce: u64,
}

#[derive(Deserialize)]
struct RawAccountTrades {
    #[serde(default, deserialize_with = "rows")]
    trades: Vec<Trade>,
    #[serde(default)]
    daily_volume: Option<Figure>,
    #[serde(default)]
    weekly_volume: Option<Figure>,
    #[serde(default)]
    monthly_volume: Option<Figure>,
    #[serde(default)]
    total_volume: Option<Figure>,
    #[serde(default)]
    daily_trades_count: Option<u64>,
    #[serde(default)]
    weekly_trades_count: Option<u64>,
    #[serde(default)]
    monthly_trades_count: Option<u64>,
    #[serde(default)]
    total_trades_count: Option<u64>,
}

#[derive(Deserialize)]
struct RawPositions {
    #[serde(default, deserialize_with = "rows")]
    positions: Vec<Position>,
    #[serde(default, deserialize_with = "rows")]
    bo_positions: Vec<BoPosition>,
    #[serde(default, deserialize_with = "rows")]
    shares: Vec<PoolShare>,
    #[serde(default)]
    last_funding_round: Option<std::collections::BTreeMap<MarketId, Dec>>,
    #[serde(default)]
    last_funding_discount: Option<std::collections::BTreeMap<MarketId, Dec>>,
    #[serde(default, deserialize_with = "whole")]
    transaction_time: u64,
}

#[derive(Deserialize)]
struct RawAssets {
    #[serde(default, deserialize_with = "assets")]
    assets: Vec<Asset>,
    #[serde(default, deserialize_with = "whole")]
    timestamp: u64,
    #[serde(default, deserialize_with = "whole")]
    transaction_time: u64,
}

#[derive(Deserialize)]
struct RawAvgEntryPrices {
    #[serde(default, deserialize_with = "rows")]
    avg_entry_prices: Vec<AvgEntryPrice>,
    #[serde(default, deserialize_with = "whole")]
    timestamp: u64,
}

#[derive(Deserialize)]
struct RawPoolData {
    #[serde(default)]
    account: Option<AccountIndex>,
    #[serde(default, deserialize_with = "rows")]
    trades: Vec<Trade>,
    #[serde(default, deserialize_with = "rows")]
    orders: Vec<Order>,
    #[serde(default, deserialize_with = "rows")]
    positions: Vec<Position>,
    #[serde(default, deserialize_with = "rows")]
    shares: Vec<PoolShare>,
    #[serde(default, deserialize_with = "rows")]
    funding_histories: Vec<PositionFunding>,
}

#[derive(Deserialize)]
struct RawPoolInfo {
    pool_info: PoolInfo,
}

#[derive(Deserialize)]
struct RawNotes {
    #[serde(default, deserialize_with = "rows")]
    notifs: Vec<Notification>,
}

#[derive(Deserialize)]
struct RawRfqs {
    #[serde(default, deserialize_with = "rows")]
    rfqs: Vec<Rfq>,
}

#[derive(Deserialize)]
struct RawPoints {
    total_live_points: Figure,
}

/// The start of a text, for an error message.
fn shown(text: &str) -> String {
    let start: String = text.chars().take(160).collect();
    match start.len() < text.len() {
        true => format!("{start}..."),
        false => start,
    }
}

#[cfg(test)]
#[path = "tests/read.rs"]
mod tests;
