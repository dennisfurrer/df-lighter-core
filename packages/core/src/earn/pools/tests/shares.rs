//! Mints and burns: held to lighter-go, refused as `Validate` refuses, and sent end to end against
//! a wire answering the venue's own recordings.

use super::*;

#[cfg(feature = "sign")]
#[test]
fn llp_mints_and_burns_sign_as_lighter_go_does() {
    use crate::earn::support::{field, fields, matches};

    for name in ["mint-llp-shares", "burn-llp-shares"] {
        let fields = fields(name);
        let pool = AccountIndex::new(field(&fields, "PublicPoolIndex")).unwrap();
        assert_eq!(pool, AccountIndex::LLP);
        let shares = field(&fields, "ShareAmount");
        match name {
            "mint-llp-shares" => matches(name, &MintShares::new(pool, shares)),
            _ => matches(name, &BurnShares::new(pool, shares)),
        }
    }
}

#[test]
fn a_pool_is_a_sub_account_and_moves_one_share_at_least() {
    assert_eq!(MintShares::new(AccountIndex::LLP, 1).check(), Ok(()));
    assert_eq!(BurnShares::new(AccountIndex::LLP, MOST_SHARES).check(), Ok(()));
    let master = AccountIndex::new(10).unwrap();
    assert_eq!(MintShares::new(master, 1).check().unwrap_err().field, "PublicPoolIndex");
    assert_eq!(BurnShares::new(AccountIndex::LLP, 0).check().unwrap_err().field, "ShareAmount");
    assert_eq!(MintShares::new(AccountIndex::LLP, MOST_SHARES + 1).check().unwrap_err().field, "ShareAmount");
}

#[test]
fn the_llp_cooldown_is_lighter_s_alone() {
    assert_eq!((llp_cooldown_ms(Network::Lighter), llp_cooldown_ms(Network::Robinhood)), (300_000, 0));
}

#[cfg(feature = "sign")]
mod sent {
    use super::*;
    use crate::earn::support::{keyed, last_sent, nonce, paths, taken};
    use crate::test_support::ok;

    const LLP: &str = include_str!("../../../../fixtures/lighter/llp.json");
    const USER_POOLS: &str = include_str!("../../../../fixtures/lighter/publicPoolsMetadata_user.json");
    const ACCOUNT_10: &str = include_str!("../../../../fixtures/lighter/account_10.json");

    fn dec(text: &str) -> Dec {
        Dec::parse(text).unwrap()
    }

    /// 100 USDC at the LLP's 94,418,596.495627 over 26,577,882,019 shares buys 28,148 of them.
    #[test]
    fn a_deposit_mints_the_whole_shares_its_collateral_buys_now() {
        let client = keyed(Network::Lighter, 100_001, [ok(LLP), nonce(55), taken("aa")]);
        let sent = crate::test_support::block_on(deposit(&client, AccountIndex::LLP, dec("100"))).unwrap();
        assert_eq!(sent.tx_hash, "aa");
        assert_eq!(
            paths(&client),
            [
                "/api/v1/publicPoolsMetadata?index=281474976710655&limit=1",
                "/api/v1/nextNonce?account_index=100001&api_key_index=4",
                "/api/v1/sendTx",
            ]
        );
        let (tx_type, info) = last_sent(&client);
        assert_eq!(tx_type, 18);
        assert_eq!(
            (info["PublicPoolIndex"].as_u64(), info["ShareAmount"].as_u64()),
            (Some(AccountIndex::LLP.get()), Some(28_148))
        );
        assert_eq!((info["AccountIndex"].as_u64(), info["Nonce"].as_u64()), (Some(100_001), Some(55)));
    }

    #[test]
    fn a_deposit_into_a_frozen_pool_or_too_small_to_buy_a_share_is_never_signed() {
        let frozen = AccountIndex::new(281_474_976_710_645).unwrap();
        let client = keyed(Network::Lighter, 100_001, [ok(USER_POOLS)]);
        let refused = crate::test_support::block_on(deposit(&client, frozen, dec("10"))).unwrap_err();
        assert!(refused.to_string().contains("frozen"), "{refused}");
        assert_eq!(paths(&client).len(), 1, "read, never sent");

        let client = keyed(Network::Lighter, 100_001, [ok(LLP)]);
        let refused = crate::test_support::block_on(deposit(&client, AccountIndex::LLP, dec("0.003"))).unwrap_err();
        assert!(refused.to_string().contains("buys no whole share"), "{refused}");
        assert_eq!(paths(&client).len(), 1);

        let unknown = AccountIndex::new(281_474_976_710_000).unwrap();
        let client = keyed(Network::Lighter, 100_001, [ok(LLP)]);
        let refused = crate::test_support::block_on(deposit(&client, unknown, dec("10"))).unwrap_err();
        assert!(refused.to_string().contains("no public pool"), "the page answered another pool: {refused}");
    }

    #[test]
    fn withdrawing_all_burns_every_share_the_account_holds() {
        let client = keyed(Network::Lighter, 10, [ok(ACCOUNT_10), nonce(7), taken("bb")]);
        crate::test_support::block_on(withdraw_all(&client, AccountIndex::LLP)).unwrap();
        assert_eq!(paths(&client)[0], "/api/v1/account?by=index&value=10");
        let (tx_type, info) = last_sent(&client);
        assert_eq!((tx_type, info["ShareAmount"].as_u64()), (19, Some(1)), "account 10 holds one LLP share");

        let elsewhere = AccountIndex::new(281_474_976_710_600).unwrap();
        let client = keyed(Network::Lighter, 10, [ok(ACCOUNT_10)]);
        let refused = crate::test_support::block_on(withdraw_all(&client, elsewhere)).unwrap_err();
        assert!(refused.to_string().contains("holds no shares"), "{refused}");
        assert_eq!(paths(&client).len(), 1);
    }

    #[test]
    fn a_withdrawal_of_shares_goes_out_once() {
        let client = keyed(Network::Robinhood, 45, [nonce(3), taken("cc")]);
        crate::test_support::block_on(withdraw(&client, AccountIndex::LLP, 1_500_000)).unwrap();
        let (tx_type, info) = last_sent(&client);
        assert_eq!((tx_type, info["ShareAmount"].as_u64()), (19, Some(1_500_000)));
        assert_eq!(paths(&client).len(), 2);
    }
}
