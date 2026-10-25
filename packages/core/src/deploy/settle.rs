//! `BinaryOptionsOperatorSettleMarket` (50): a binary options book settled by its operator, at a
//! price or as a refund. Open orders are cancelled and every position paid to its USDC balance:
//! an Up share the settlement price, a Down share the payout cap less it, both in price ticks.
//!
//! A discrete book settles at 0 (Down) or its payout cap (Up); a continuous one anywhere between.
//! A refund settles at the book's default price, fixed at creation, and carries price 0.

use crate::deploy::slot::{MarketSlot, binary_options_slot};
use crate::utils::MarketId;
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{BadTx, Envelope, Tx, TxType, element, number};

/// `BinaryOptionsOperatorSettleMarket`, as lighter-go's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettleMarket {
    pub slot: MarketSlot,
    /// The id traders know the book by.
    pub market: MarketId,
    /// In price ticks, 0 to the payout cap (the venue holds the cap); 0 for a refund.
    pub price: u32,
    /// Settles every position at the default price instead.
    pub refund: bool,
}

impl SettleMarket {
    /// At `price` ticks.
    #[must_use]
    pub const fn at(slot: MarketSlot, market: MarketId, price: u32) -> SettleMarket {
        SettleMarket { slot, market, price, refund: false }
    }

    /// Up: Up shares take the whole `settlement_cap`.
    #[must_use]
    pub const fn up(slot: MarketSlot, market: MarketId, settlement_cap: u32) -> SettleMarket {
        SettleMarket::at(slot, market, settlement_cap)
    }

    /// Down: Down shares take the whole payout.
    #[must_use]
    pub const fn down(slot: MarketSlot, market: MarketId) -> SettleMarket {
        SettleMarket::at(slot, market, 0)
    }

    /// No outcome: every position settles at the default price.
    #[must_use]
    pub const fn refund(slot: MarketSlot, market: MarketId) -> SettleMarket {
        SettleMarket { slot, market, price: 0, refund: true }
    }
}

impl Tx for SettleMarket {
    fn tx_type(&self) -> TxType {
        TxType::BinaryOptionsSettleMarket
    }

    /// Robinhood Chain lists no binary options.
    fn check_signing(&self, chain_id: u32, _envelope: &Envelope) -> Result<(), BadTx> {
        crate::deploy::check_chain(chain_id)
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![element(self.slot.get()), element(self.market.get()), element(self.price), element(u8::from(self.refund))]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![
            ("MarketIndex", number(self.slot.get())),
            ("PublicMarketIndex", number(self.market.get())),
            ("SettlementPrice", number(self.price)),
            ("IsRefund", number(u8::from(self.refund))),
        ]
    }

    /// lighter-go's `Validate`.
    fn check(&self) -> Result<(), BadTx> {
        binary_options_slot(self.slot)?;
        if self.refund && self.price != 0 {
            return Err(BadTx::new("SettlementPrice", "a refund carries price 0: it settles at the default price"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/settle.rs"]
mod tests;
