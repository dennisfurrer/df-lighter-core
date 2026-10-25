//! Moving money: between an owner's own accounts with the key alone, to another owner only with
//! the owner's signature, and out to the owner's L1 address.

use df_lighter_core::SendError;
use df_lighter_core::account::LocalWallet;
use df_lighter_core::client::{ApiError, Network};
use df_lighter_core::transfer::{self, Transfer};
use df_lighter_core::utils::AccountIndex;

use crate::client::trader;
use crate::support::venue::Venue;
use crate::support::{ACCOUNT, owner, owner_address};

const SUB: u64 = 281_474_976_710_600;
const OTHER: u64 = 713_888;

fn venue() -> Venue {
    let stranger = LocalWallet::from_hex("0x0000000000000000000000000000000000000000000000000000000000000002").unwrap();
    Venue::lighter()
        .account(ACCOUNT, owner_address())
        .account(SUB, owner_address())
        .account(OTHER, stranger.address())
        .text("transferFeeInfo", r#"{"code":200,"transfer_fee_usdc":1000000}"#)
}

#[tokio::test]
async fn a_transfer_to_the_owner_s_own_account_needs_only_the_key() {
    let client = trader(venue());
    let to = AccountIndex::new(SUB).unwrap();
    transfer::transfer(&client, Transfer::collateral(Network::Lighter, to, "5".parse().unwrap()).unwrap())
        .await
        .unwrap();
    let taken = client.http().taken();
    assert_eq!((taken[0].tx_type, taken[0].number("ToAccountIndex"), taken[0].number("Amount")), (12, SUB, 5_000_000));
    assert_eq!(taken[0].number("USDCFee"), 1_000_000, "the fee the venue asked");
}

/// To another owner the venue wants the owner's `personal_sign`: without it the transfer is
/// refused, with it taken.
#[tokio::test]
async fn a_transfer_to_another_owner_carries_the_owner_s_signature() {
    let client = trader(venue());
    let to = AccountIndex::new(OTHER).unwrap();
    let send = || Transfer::collateral(Network::Lighter, to, "100".parse().unwrap()).unwrap();
    let unsigned = transfer::transfer(&client, send()).await.unwrap_err();
    assert!(matches!(unsigned, SendError::Api(ApiError::Refused { code: 21_120, .. })), "{unsigned}");
    transfer::transfer_to_owner(&client, send(), &owner()).await.unwrap();
    let taken = client.http().taken();
    assert_eq!(taken.len(), 1);
    assert!(taken[0].info.get("L1Sig").and_then(|sig| sig.as_str()).is_some_and(|sig| sig.len() == 132));
}

#[tokio::test]
async fn a_withdrawal_goes_to_the_owner_s_l1_address() {
    let client = trader(venue());
    transfer::withdraw(&client, "12.34".parse().unwrap()).await.unwrap();
    let taken = client.http().taken();
    assert_eq!(
        (taken[0].tx_type, taken[0].number("FromAccountIndex"), taken[0].number("Amount")),
        (13, ACCOUNT, 12_340_000)
    );
}
