//! Stakes and unstakes: held to lighter-go, refused as `Validate` refuses, and sent end to end
//! against a wire answering the staking pool as Lighter listed it.

use super::*;

#[cfg(feature = "sign")]
#[test]
fn lit_stakes_and_unstakes_sign_as_lighter_go_does() {
    use crate::earn::support::{field, fields, matches};

    for name in ["stake-lit", "unstake-lit"] {
        let fields = fields(name);
        let pool = AccountIndex::new(field(&fields, "StakingPoolIndex")).unwrap();
        assert_eq!(pool, LIGHTER_STAKING_POOL);
        let shares = field(&fields, "ShareAmount");
        match name {
            "stake-lit" => matches(name, &StakeAssets::new(pool, shares)),
            _ => matches(name, &UnstakeAssets::new(pool, shares)),
        }
    }
}

#[test]
fn only_lighter_runs_a_staking_pool() {
    assert_eq!(staking_pool_index(Network::Lighter), Some(LIGHTER_STAKING_POOL));
    assert_eq!(staking_pool_index(Network::Robinhood), None);
    assert_eq!(STAKING_LOCKUP_MS, 3 * crate::utils::time::DAY_MS);
}

#[test]
fn a_staking_pool_is_a_sub_account_and_moves_one_share_at_least() {
    assert_eq!(StakeAssets::new(LIGHTER_STAKING_POOL, 1).check(), Ok(()));
    let master = AccountIndex::new(10).unwrap();
    assert_eq!(StakeAssets::new(master, 1).check().unwrap_err().field, "StakingPoolIndex");
    assert_eq!(UnstakeAssets::new(master, 1).check().unwrap_err().field, "StakingPoolIndex");
    assert_eq!(UnstakeAssets::new(LIGHTER_STAKING_POOL, 0).check().unwrap_err().field, "ShareAmount");
}

#[cfg(feature = "sign")]
mod sent {
    use super::*;
    use crate::earn::support::{keyed, last_sent, nonce, paths, taken};
    use crate::test_support::{block_on, ok};

    const STAKE: &str = include_str!("../../../../fixtures/lighter/publicPoolsMetadata_stake.json");

    /// 100 LIT at 111,639,831.81372573 over 10,608,033,680,396 shares buys 9,502,015 of them.
    #[test]
    fn a_stake_buys_the_whole_shares_its_lit_buys_now() {
        let client = keyed(Network::Lighter, 100_001, [ok(STAKE), nonce(57), taken("aa")]);
        block_on(stake(&client, Dec::from_int(100))).unwrap();
        assert_eq!(
            paths(&client)[0],
            "/api/v1/publicPoolsMetadata?filter=stake&index=281474976624801&limit=1",
            "the staking pool is listed under its filter only"
        );
        let (tx_type, info) = last_sent(&client);
        assert_eq!(tx_type, 35);
        assert_eq!(info["StakingPoolIndex"].as_u64(), Some(LIGHTER_STAKING_POOL.get()));
        assert_eq!(info["ShareAmount"].as_u64(), Some(9_502_015));
    }

    #[test]
    fn nothing_is_staked_or_unstaked_on_robinhood_chain() {
        let client = keyed(Network::Robinhood, 45, []);
        let refused = block_on(stake(&client, Dec::from_int(100))).unwrap_err();
        assert!(refused.to_string().contains("runs no staking pool"), "{refused}");
        assert!(block_on(unstake(&client, 1)).is_err());
        assert!(client.http().sent().is_empty(), "nothing asked, nothing sent");
    }

    #[test]
    fn an_unstake_sells_shares_of_the_network_s_pool() {
        let client = keyed(Network::Lighter, 100_001, [nonce(58), taken("bb")]);
        block_on(unstake(&client, 40_000_000)).unwrap();
        let (tx_type, info) = last_sent(&client);
        assert_eq!((tx_type, info["ShareAmount"].as_u64()), (36, Some(40_000_000)));
        assert_eq!(info["StakingPoolIndex"].as_u64(), Some(LIGHTER_STAKING_POOL.get()));
    }

    #[test]
    fn a_stake_too_small_for_a_share_is_never_signed() {
        let client = keyed(Network::Lighter, 100_001, [ok(STAKE)]);
        let refused = block_on(stake(&client, Dec::parse("0.00001").unwrap())).unwrap_err();
        assert!(refused.to_string().contains("buys no whole staking share"), "{refused}");
        assert_eq!(paths(&client).len(), 1);
    }
}
