//! Operator transactions go out on Lighter and are refused on Robinhood Chain before anything is
//! signed.

use super::*;

#[test]
fn only_lighter_takes_operator_transactions() {
    assert_eq!(check_network(Network::Lighter), Ok(()));
    assert_eq!(check_network(Network::Robinhood).unwrap_err().field, "Network");
}

#[cfg(feature = "sign")]
mod sent {
    use super::*;
    use crate::earn::support::{keyed, last_sent, nonce, paths, taken};
    use crate::test_support::block_on;
    use crate::utils::MarketId;

    fn up() -> SettleMarket {
        SettleMarket::up(MarketSlot::of::<1500>(), MarketId::of::<4206>(), 100)
    }

    #[test]
    fn a_settlement_goes_out_once_on_lighter() {
        let client = keyed(Network::Lighter, 100_001, [nonce(65), taken("aa")]);
        let sent = block_on(send_operator(&client, &up())).unwrap();
        assert_eq!(sent.tx_hash, "aa");
        let (tx_type, info) = last_sent(&client);
        assert_eq!(tx_type, 50);
        assert_eq!((info["MarketIndex"].as_u64(), info["PublicMarketIndex"].as_u64()), (Some(1500), Some(4206)));
        assert_eq!(paths(&client).len(), 2);
    }

    #[test]
    fn robinhood_chain_refuses_before_a_nonce_is_asked() {
        let client = keyed(Network::Robinhood, 45, [nonce(1), taken("aa")]);
        let refused = block_on(send_operator(&client, &up())).unwrap_err();
        assert!(refused.to_string().contains("lists no binary options"), "{refused}");
        assert!(client.http().sent().is_empty());
    }
}
