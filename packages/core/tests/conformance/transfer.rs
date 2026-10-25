//! Moving money, built from the Go fields lighter-go was handed: transfers to a sub-account, to
//! another owner with a fee, to the fast withdrawal pool with the owner's address as the memo, of
//! spot ETH, from perps to spot and on Robinhood Chain, each with the text the owner signs and the
//! `tx_info` sent with that signature; a transfer of nothing refused; secure withdrawals of USDC,
//! of spot ETH and of USDG, which the API key signs alone.

use df_lighter_core::transfer::{Memo, Transfer, Withdraw};
use df_lighter_core::utils::hex;

use crate::support::owner_address;
use crate::vector::{Refused, cases, every};

const TRANSFER: [&str; 7] = [
    "transfer-to-sub-account",
    "transfer-to-another-owner",
    "fast-withdraw-memo",
    "transfer-spot-eth",
    "transfer-perps-to-spot",
    "robinhood-transfer",
    "refused-transfer-zero",
];
const WITHDRAW: [&str; 3] = ["withdraw-usdc-perps", "withdraw-eth-spot", "robinhood-withdraw"];

/// The transaction cases replayed here.
pub const CASES: [&[&str]; 2] = [&TRANSFER, &WITHDRAW];

#[test]
fn every_transfer_signs_as_lighter_go_signs_it_with_the_owner_s_signature() {
    let refused = [("refused-transfer-zero", Refused::Field("Amount"))];
    every("Transfer", &TRANSFER, &refused, |case| {
        let fields = case.fields();
        Ok(Transfer {
            to: fields.account("ToAccountIndex")?,
            asset: fields.asset("AssetIndex")?,
            from_route: fields.route("FromRouteType"),
            to_route: fields.route("ToRouteType"),
            amount: fields.int("Amount"),
            fee: fields.int("USDCFee"),
            memo: Memo::from_bytes(hex::decode_array::<32>(fields.text("Memo")).unwrap()),
        })
    });
}

#[test]
fn a_fast_withdrawal_s_memo_is_the_owner_s_address() {
    let case = cases("Transfer").into_iter().find(|case| case.name() == "fast-withdraw-memo").unwrap();
    assert_eq!(Memo::address(owner_address()).to_hex(), case.fields().text("Memo"));
}

#[test]
fn every_secure_withdrawal_signs_as_lighter_go_signs_it_by_the_key_alone() {
    every("Withdraw", &WITHDRAW, &[], |case| {
        let fields = case.fields();
        Ok(Withdraw {
            asset: fields.asset("AssetIndex")?,
            route: fields.route("RouteType"),
            amount: fields.int("Amount"),
        })
    });
}
