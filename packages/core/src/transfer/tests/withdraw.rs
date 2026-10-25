//! Secure withdrawals held to lighter-go (`vectors/lighter_go.json`): the hash and `tx_info`; and
//! amounts made exactly.

use super::*;

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn amounts_are_made_exactly_and_nothing_is_refused() {
    let usdg = Withdraw::collateral(Network::Robinhood, dec("1")).unwrap();
    assert_eq!(usdg, Withdraw { asset: AssetId::COLLATERAL, route: Route::Perps, amount: 1_000_000 });
    let eth = Withdraw::new(AssetId::ETH, 8, Route::Spot, dec("0.5")).unwrap();
    assert_eq!((eth.amount, eth.route), (50_000_000, Route::Spot));
    assert!(matches!(Withdraw::collateral(Network::Lighter, dec("0.0000001")), Err(BadUnits::TooFine { .. })));
    assert!(Withdraw { amount: 0, ..usdg }.check().is_err());
    assert!(Withdraw { amount: MAX_AMOUNT + 1, ..usdg }.check().is_err());
    assert_eq!(Withdraw { amount: MAX_AMOUNT, ..usdg }.check(), Ok(()));
    let account = crate::utils::AccountIndex::new(1).unwrap();
    let api_key = crate::utils::ApiKeyIndex::new(4).unwrap();
    let envelope = crate::utils::sign::Envelope { account, api_key, nonce: 0, expired_at: 0 };
    assert_eq!(usdg.l1_message(&envelope, 304), None, "the API key alone signs a withdrawal");
}

#[cfg(feature = "sign")]
#[test]
fn withdrawals_sign_as_lighter_go_does() {
    use crate::test_support::{replay, vector, vector_envelope, vector_key};
    use crate::utils::sign::tx::sign_drawing;

    for name in ["withdraw-usdc-perps", "withdraw-eth-spot", "robinhood-withdraw"] {
        let case = vector(name);
        let (chain_id, envelope) = vector_envelope(&case);
        let fields = &case["fields"];
        let tx = Withdraw {
            asset: AssetId::new(u16::try_from(fields["AssetIndex"].as_u64().unwrap()).unwrap()).unwrap(),
            route: if fields["RouteType"] == 0 { Route::Perps } else { Route::Spot },
            amount: fields["Amount"].as_u64().unwrap(),
        };
        let signed = sign_drawing(&tx, chain_id, &envelope, &vector_key(&case), replay(&case)).unwrap();
        let out = &case["out"];
        assert_eq!(u64::from(signed.tx_type().code()), out["tx_type"].as_u64().unwrap(), "{name}: type");
        assert_eq!(signed.hash().to_hex(), out["tx_hash"].as_str().unwrap(), "{name}: hash");
        assert_eq!(signed.tx_info(), out["tx_info"].as_str().unwrap(), "{name}: tx_info");
        assert_eq!(signed.l1_message(), None, "{name}: no owner's signature");
    }
}
