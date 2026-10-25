//! Integrator approvals, held to lighter-go and go-ethereum (`vectors/lighter_go.json`): the hash,
//! `tx_info`, the text the owner signs, the owner's signature and the `tx_info` sent; lighter-go's
//! refusals; and the approvals a recorded account lists.

use super::*;

/// A case's approval, from its Go field names.
fn approval(case: &serde_json::Value) -> ApproveIntegrator {
    let fields = &case["fields"];
    let fee = |name: &str| u32::try_from(fields[name].as_u64().unwrap()).unwrap();
    ApproveIntegrator {
        integrator: AccountIndex::new(fields["IntegratorAccountIndex"].as_u64().unwrap()).unwrap(),
        max_perps_taker_fee: fee("MaxPerpsTakerFee"),
        max_perps_maker_fee: fee("MaxPerpsMakerFee"),
        max_spot_taker_fee: fee("MaxSpotTakerFee"),
        max_spot_maker_fee: fee("MaxSpotMakerFee"),
        expiry: fields["ApprovalExpiry"].as_u64().unwrap(),
    }
}

#[cfg(feature = "sign")]
#[test]
fn approvals_and_revocations_sign_and_carry_the_owner_s_signature_as_lighter_go_does() {
    use crate::account::keys::LocalWallet;
    use crate::test_support::{replay, vector, vector_envelope, vector_key, vector_wallet_key};
    use crate::utils::sign::tx::sign_drawing;

    let owner = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
    for name in ["approve-integrator", "revoke-integrator", "robinhood-approve-integrator"] {
        let case = vector(name);
        let (chain_id, envelope) = vector_envelope(&case);
        let tx = approval(&case);
        let signed = sign_drawing(&tx, chain_id, &envelope, &vector_key(&case), replay(&case)).unwrap();
        let out = &case["out"];
        assert_eq!(signed.tx_type().code(), 45, "{name}");
        assert_eq!(signed.hash().to_hex(), out["tx_hash"].as_str().unwrap(), "{name}: hash");
        assert_eq!(signed.tx_info(), out["tx_info"].as_str().unwrap(), "{name}: tx_info");
        assert_eq!(signed.l1_message(), out["message_to_sign"].as_str(), "{name}: message");
        let l1 = owner.sign_text(signed.l1_message().unwrap()).unwrap();
        assert_eq!(l1.to_hex(), out["l1_sig"].as_str().unwrap(), "{name}: the owner's signature");
        assert_eq!(signed.tx_info_with_l1(&l1.to_hex()).as_deref(), out["signed_tx_info"].as_str(), "{name}");
    }
}

#[test]
fn a_revocation_is_every_fee_zero_and_needs_no_owner() {
    let integrator = AccountIndex::new(713_888).unwrap();
    let revoke = ApproveIntegrator::revoke(integrator);
    assert!(revoke.is_revocation() && !revoke.charges() && !revoke.needs_owner());
    assert_eq!(revoke.check(), Ok(()));
    let tracking = ApproveIntegrator::new(integrator, 0, 0, 1_821_536_000_000);
    assert!(!tracking.is_revocation() && !tracking.needs_owner(), "tracking only is the key's alone");
    let charging = ApproveIntegrator::new(integrator, 1_000, 10_000, 1_821_536_000_000);
    assert!(charging.charges() && charging.needs_owner());
    assert_eq!((charging.max_perps_maker_fee, charging.max_spot_taker_fee), (1_000, 10_000));
    assert!(charging.trailing_l1_sig());
}

#[test]
fn lighter_go_s_refusals_are_refused_before_signing() {
    let integrator = AccountIndex::new(713_888).unwrap();
    let too_high = ApproveIntegrator::new(integrator, FEE_TICK + 1, 0, 1_821_536_000_000);
    assert_eq!(too_high.check().unwrap_err().field, "MaxFee");
    let fees_without_expiry = ApproveIntegrator::new(integrator, 500, 0, 0);
    assert_eq!(fees_without_expiry.check().unwrap_err().field, "ApprovalExpiry");
    let past_48_bits = ApproveIntegrator::new(integrator, 500, 0, WIDTH_48 + 1);
    assert_eq!(past_48_bits.check().unwrap_err().field, "ApprovalExpiry");
    let all_of_it = ApproveIntegrator::new(integrator, FEE_TICK, FEE_TICK, WIDTH_48);
    assert_eq!(all_of_it.check(), Ok(()));
}

#[test]
fn a_recorded_account_lists_its_approved_integrators() {
    let page: serde_json::Value =
        serde_json::from_str(include_str!("../../../../fixtures/lighter/account_10.json")).unwrap();
    let listed: Vec<ApprovedIntegrator> =
        serde_json::from_value(page["accounts"][0]["approved_integrators"].clone()).unwrap();
    let lighter = &listed[0];
    assert_eq!((lighter.integrator.get(), lighter.name.as_str()), (281_474_976_496_476, "Lighter"));
    assert_eq!((lighter.max_perps_taker_fee, lighter.max_spot_maker_fee), (1_000, 10_000));
    assert!(lighter.is_active(1_790_000_000_000) && !lighter.is_active(1_819_163_972_113));
    assert_eq!(ApprovedIntegrator::fraction(lighter.max_perps_taker_fee).to_string(), "0.001", "ten basis points");
}
