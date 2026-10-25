//! The two exchanges Lighter runs: Lighter itself, settled on Ethereum with USDC, and Lighter on
//! Robinhood Chain, with USDG. They share the software, the transaction format and the API, and
//! nothing else: accounts, keys, nonces, market ids and balances are each venue's own, and a
//! transaction signed for one is refused by the other. Everything that differs is a method on
//! [`Network`], so the network is never assumed.
//!
//! Sources: each host's `layer1BasicInfo` and `info` (the contract, the collateral token), the
//! docs' environments and Robinhood Chain pages, and lighter-ts-sdk 1.1.0 for the signing chain
//! ids (Robinhood's is neither its L1's chain id nor its network id).

use std::{fmt, str::FromStr};

use crate::utils::{Address, ApiKeyIndex, AssetId};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Network {
    /// Lighter: settled on Ethereum, USDC collateral.
    #[default]
    Lighter,
    /// Lighter on Robinhood Chain: settled on Robinhood Chain, USDG collateral; crypto, US stocks,
    /// pre-IPO names and metals.
    Robinhood,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a Lighter network; expected lighter or robinhood")]
pub struct UnknownNetwork(pub String);

/// The collateral perps margin in, as each place names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Collateral {
    pub symbol: &'static str,
    /// The ERC-20 on the network's L1.
    pub token: Address,
    /// The token's decimals on its L1, which a deposit's amount is counted in.
    pub l1_decimals: u32,
    /// The asset id the venue knows it by.
    pub asset: AssetId,
    /// The decimals the venue counts it in, which a transfer or a withdrawal is signed in.
    pub decimals: u32,
}

/// USDC on Ethereum.
pub const USDC: Collateral = Collateral {
    symbol: "USDC",
    token: address("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"),
    l1_decimals: 6,
    asset: AssetId::COLLATERAL,
    decimals: 6,
};

/// USDG on Robinhood Chain.
pub const USDG: Collateral = Collateral {
    symbol: "USDG",
    token: address("0x5fc5360D0400a0Fd4f2af552ADD042D716F1d168"),
    l1_decimals: 6,
    asset: AssetId::COLLATERAL,
    decimals: 6,
};

/// The ZkLighter contract on Ethereum: deposits, secure withdrawals, priority transactions.
pub const LIGHTER_CONTRACT: Address = address("0x3B4D794a66304F130a4Db8F2551B0070dfCf5ca7");

/// The ZkLighter contract on Robinhood Chain.
pub const ROBINHOOD_CONTRACT: Address = address("0x94bAB9693Ba2f6358507eFfcbd372b0660AFfF9d");

const fn address(text: &str) -> Address {
    match Address::parse_const(text) {
        Some(address) => address,
        None => Address::ZERO,
    }
}

impl Network {
    pub const ALL: [Network; 2] = [Network::Lighter, Network::Robinhood];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Network::Lighter => "lighter",
            Network::Robinhood => "robinhood",
        }
    }

    /// The REST API's host; every path is under `/api/v1/`.
    #[must_use]
    pub const fn api_url(self) -> &'static str {
        match self {
            Network::Lighter => "https://mainnet.zklighter.elliot.ai",
            Network::Robinhood => "https://api.rh.lighter.xyz",
        }
    }

    /// The websocket. `?readonly=true` serves market data in restricted regions.
    #[must_use]
    pub const fn ws_url(self) -> &'static str {
        match self {
            Network::Lighter => "wss://mainnet.zklighter.elliot.ai/stream",
            Network::Robinhood => "wss://api.rh.lighter.xyz/stream",
        }
    }

    /// The read-only websocket, which takes no transactions.
    #[must_use]
    pub const fn ws_readonly_url(self) -> &'static str {
        match self {
            Network::Lighter => "wss://mainnet.zklighter.elliot.ai/stream?readonly=true",
            Network::Robinhood => "wss://api.rh.lighter.xyz/stream?readonly=true",
        }
    }

    /// The chain id every transaction is signed for: not a real chain's.
    #[must_use]
    pub const fn chain_id(self) -> u32 {
        match self {
            Network::Lighter => 304,
            Network::Robinhood => 466_324,
        }
    }

    /// The chain the venue settles on, where deposits are made and secure withdrawals arrive.
    #[must_use]
    pub const fn l1_chain_id(self) -> u64 {
        match self {
            Network::Lighter => 1,
            Network::Robinhood => 4663,
        }
    }

    #[must_use]
    pub const fn l1_name(self) -> &'static str {
        match self {
            Network::Lighter => "Ethereum",
            Network::Robinhood => "Robinhood Chain",
        }
    }

    /// A public JSON-RPC node on the L1, for reads and to send a deposit through. A program in
    /// production brings its own.
    #[must_use]
    pub const fn l1_rpc_url(self) -> &'static str {
        match self {
            Network::Lighter => "https://ethereum-rpc.publicnode.com",
            Network::Robinhood => "https://rpc.mainnet.chain.robinhood.com",
        }
    }

    /// The L1's block explorer, for a link a person follows.
    #[must_use]
    pub const fn l1_explorer_url(self) -> &'static str {
        match self {
            Network::Lighter => "https://etherscan.io",
            Network::Robinhood => "https://robinhoodchain.blockscout.com",
        }
    }

    /// The ZkLighter contract on the L1.
    #[must_use]
    pub const fn contract(self) -> Address {
        match self {
            Network::Lighter => LIGHTER_CONTRACT,
            Network::Robinhood => ROBINHOOD_CONTRACT,
        }
    }

    #[must_use]
    pub const fn collateral(self) -> Collateral {
        match self {
            Network::Lighter => USDC,
            Network::Robinhood => USDG,
        }
    }

    /// The explorer's API: blocks, batches, logs and an account's history.
    #[must_use]
    pub const fn explorer_api_url(self) -> &'static str {
        match self {
            Network::Lighter => "https://explorer.elliot.ai/api",
            Network::Robinhood => "https://explorerapi.rh.lighter.xyz/api",
        }
    }

    /// The venue's own web app, for a link a person follows.
    #[must_use]
    pub const fn app_url(self) -> &'static str {
        match self {
            Network::Lighter => "https://app.lighter.xyz",
            Network::Robinhood => "https://lighter.xyz",
        }
    }

    /// Whether the slot belongs to Lighter's own web and mobile apps: 0 to 3 on both venues, and
    /// 157 too on Robinhood Chain. Registering a key there logs the venue's app out.
    #[must_use]
    pub const fn reserves_api_key(self, slot: ApiKeyIndex) -> bool {
        matches!((self, slot.get()), (_, 0..=3) | (Network::Robinhood, 157))
    }

    /// Whether the venue lists binary options: Lighter does, Robinhood Chain does not.
    #[must_use]
    pub const fn has_binary_options(self) -> bool {
        matches!(self, Network::Lighter)
    }

    /// Fast withdrawals an L1 address may make an hour.
    #[must_use]
    pub const fn fast_withdrawals_per_hour(self) -> u32 {
        match self {
            Network::Lighter => 10,
            Network::Robinhood => 20,
        }
    }

    /// Sub-accounts a Standard account may hold: 4 on Lighter, 8 on Robinhood Chain.
    #[must_use]
    pub const fn standard_sub_accounts(self) -> u32 {
        match self {
            Network::Lighter => 4,
            Network::Robinhood => 8,
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Network {
    type Err = UnknownNetwork;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "lighter" | "zklighter" | "mainnet" => Ok(Network::Lighter),
            "robinhood" | "lighter-rh" | "rh" => Ok(Network::Robinhood),
            _ => Err(UnknownNetwork(text.trim().to_owned())),
        }
    }
}

#[cfg(test)]
#[path = "tests/network.rs"]
mod tests;
