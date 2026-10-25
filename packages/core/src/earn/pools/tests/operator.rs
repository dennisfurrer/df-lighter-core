//! Opening and updating a pool: held to lighter-go (`vectors/lighter_go.json`), the rates made
//! from decimals, and what `Validate` refuses.

use super::*;

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[cfg(feature = "sign")]
#[test]
fn a_new_pool_and_its_update_sign_as_lighter_go_does() {
    use crate::earn::support::{field, fields, matches};

    let create = fields("create-public-pool");
    let tx = CreatePublicPool {
        operator_fee: u32::try_from(field(&create, "OperatorFee")).unwrap(),
        initial_total_shares: field(&create, "InitialTotalShares"),
        min_operator_share_rate: u16::try_from(field(&create, "MinOperatorShareRate")).unwrap(),
    };
    assert_eq!(tx, CreatePublicPool::new(dec("0.1"), dec("1000000"), dec("0.1")).unwrap(), "10%, a million, 10%");
    matches("create-public-pool", &tx);

    let update = fields("update-public-pool");
    let tx = UpdatePublicPool {
        pool: AccountIndex::new(field(&update, "PublicPoolIndex")).unwrap(),
        status: PoolStatus::from_code(u8::try_from(field(&update, "Status")).unwrap()),
        operator_fee: u32::try_from(field(&update, "OperatorFee")).unwrap(),
        min_operator_share_rate: u16::try_from(field(&update, "MinOperatorShareRate")).unwrap(),
    };
    assert_eq!(tx.status, PoolStatus::Frozen);
    assert_eq!(tx, UpdatePublicPool::new(tx.pool, PoolStatus::Frozen, dec("0.05"), dec("0.05")).unwrap());
    matches("update-public-pool", &tx);
}

#[test]
fn rates_are_made_exactly_and_refused_when_finer_than_the_venue_counts() {
    assert_eq!(operator_fee_units(dec("0")), Ok(0), "no fee is a fee");
    assert_eq!(operator_fee_units(dec("0.000001")), Ok(1));
    assert_eq!(operator_fee_units(dec("1")), Ok(FEE_TICK));
    assert!(matches!(operator_fee_units(dec("0.0000001")), Err(BadUnits::TooFine { places: 6, .. })));
    assert!(matches!(operator_fee_units(dec("1.000001")), Err(BadUnits::TooLarge { .. })));
    assert!(matches!(operator_fee_units(dec("-0.1")), Err(BadUnits::NotPositive(_))));
    assert_eq!(min_operator_share_units(dec("0.01")), Ok(100), "1% of the pool");
    assert!(matches!(min_operator_share_units(dec("0.00001")), Err(BadUnits::TooFine { places: 4, .. })));

    let pool = CreatePublicPool::new(dec("0"), dec("1000"), dec("0")).unwrap();
    assert_eq!(pool.initial_total_shares, MIN_INITIAL_TOTAL_SHARES, "1,000 collateral at 0.001 a share");
    assert!(matches!(CreatePublicPool::new(dec("0"), dec("1000.0005"), dec("0")), Err(BadUnits::TooFine { .. })));
    assert!(matches!(CreatePublicPool::new(dec("0"), dec("0"), dec("0")), Err(BadUnits::NotPositive(_))));
}

#[test]
fn what_lighter_go_refuses_is_refused_before_signing() {
    let good = CreatePublicPool { operator_fee: 0, initial_total_shares: 1, min_operator_share_rate: 0 };
    assert_eq!(good.check(), Ok(()), "lighter-go asks one share; the venue asks a thousand collateral");
    let field = |tx: CreatePublicPool| tx.check().unwrap_err().field;
    assert_eq!(field(CreatePublicPool { operator_fee: FEE_TICK + 1, ..good }), "OperatorFee");
    assert_eq!(field(CreatePublicPool { initial_total_shares: 0, ..good }), "InitialTotalShares");
    assert_eq!(
        field(CreatePublicPool { initial_total_shares: MAX_INITIAL_TOTAL_SHARES + 1, ..good }),
        "InitialTotalShares"
    );
    assert_eq!(field(CreatePublicPool { min_operator_share_rate: SHARE_TICK + 1, ..good }), "MinOperatorShareRate");

    let update = UpdatePublicPool {
        pool: AccountIndex::LLP,
        status: PoolStatus::Active,
        operator_fee: FEE_TICK,
        min_operator_share_rate: SHARE_TICK,
    };
    assert_eq!(update.check(), Ok(()));
    let field = |tx: UpdatePublicPool| tx.check().unwrap_err().field;
    assert_eq!(field(UpdatePublicPool { status: PoolStatus::Other(2), ..update }), "Status");
    assert_eq!(field(UpdatePublicPool { operator_fee: FEE_TICK + 1, ..update }), "OperatorFee");
    assert_eq!(field(UpdatePublicPool { min_operator_share_rate: SHARE_TICK + 1, ..update }), "MinOperatorShareRate");
}

#[test]
fn a_status_reads_by_its_number_and_an_unknown_one_is_kept() {
    let read = |text: &str| serde_json::from_str::<PoolStatus>(text).unwrap();
    assert_eq!((read("0"), read("1"), read("7")), (PoolStatus::Active, PoolStatus::Frozen, PoolStatus::Other(7)));
    assert_eq!(PoolStatus::Other(7).code(), 7);
    assert_eq!(INITIAL_SHARE_PRICE.to_string(), "0.001");
}
