//! Each venue's weights, the tier's budget, the minute's meter and the gate a 429 or a 405
//! closes, each handed the time.

use super::*;

#[test]
fn a_path_weighs_what_its_venue_s_table_says() {
    for (path, lighter, robinhood) in [
        ("sendTx", 6, 6),
        ("sendTxBatch", 6, 6),
        ("nextNonce", 6, 6),
        ("tv/history", 10, 10),
        ("publicPoolsMetadata", 50, 50),
        ("accountActiveOrders", 100, 100),
        ("apikeys", 150, 150),
        ("account", 300, 150),
        ("trades", 200, 600),
        ("recentTrades", 600, 600),
        ("transferFeeInfo", 500, 500),
        ("orderBooks", 300, 300),
        ("somethingNew", 300, 300),
        ("tokens/create", 10_000, 23_000),
        ("changeAccountTier", 3_000, 3_000),
    ] {
        assert_eq!(weight(Network::Lighter, path), lighter, "{path} on Lighter");
        assert_eq!(weight(Network::Robinhood, path), robinhood, "{path} on Robinhood Chain");
    }
    assert!(is_send("sendTx") && is_send("sendTxBatch") && is_send("fastwithdraw") && is_send("litLease"));
    assert!(!is_send("nextNonce"));
}

/// Standard is sixty requests a minute however light, and no more of a heavy one than 24,000
/// weight allows; the paid tiers spend weight.
#[test]
fn a_standard_request_costs_at_least_a_sixtieth_of_the_minute() {
    assert_eq!(Tier::Standard.budget(Network::Lighter) / Tier::Standard.cost(6), 60);
    assert_eq!(Tier::Standard.budget(Network::Lighter) / Tier::Standard.cost(600), 40);
    assert_eq!((Tier::Plus.cost(6), Tier::Premium.cost(600)), (6, 600));
    for (tier, lighter, robinhood) in [
        (Tier::Standard, 24_000, 24_000),
        (Tier::Plus, 24_000, 120_000),
        (Tier::Premium, 24_000, 24_000),
        (Tier::Builder, 240_000, 150_000),
    ] {
        assert_eq!((tier.budget(Network::Lighter), tier.budget(Network::Robinhood)), (lighter, robinhood), "{tier:?}");
    }
    assert!(Tier::Standard.sends_share_reads() && !Tier::Plus.sends_share_reads());
    assert_eq!(Tier::Premium.name(), "premium");
}

#[test]
fn reads_wait_once_the_minute_nears_four_fifths_of_the_budget() {
    let mut meter = Meter::default();
    for second in 0..47 {
        meter.record(second * 1_000, "orderBooks", 400);
    }
    // 18,800 spent of a soft 19,200: one more read of 400 fits, two do not.
    assert_eq!(meter.wait_for(47_000, 400, STANDARD_BUDGET), None);
    assert_eq!(meter.wait_for(47_000, 800, STANDARD_BUDGET), Some(13_000), "until the first read rolls off");
    assert_eq!(meter.wait_for(47_000, 1_200, STANDARD_BUDGET), Some(14_000));
    assert_eq!(meter.wait_for(47_000, 30_000, STANDARD_BUDGET), Some(WINDOW_MS), "more than a minute holds");
    assert_eq!(meter.wait_for(61_000, 800, STANDARD_BUDGET), None, "a minute on, room again");
}

#[test]
fn the_minute_adds_up_by_path_heaviest_first_and_then_forgets() {
    let mut meter = Meter::default();
    meter.record(0, "orderBooks", 400);
    meter.record(10, "orderBooks", 400);
    meter.record(20, "recentTrades", 600);
    meter.record(30, "sendTx", 400);
    meter.refuse(40);
    let spent = meter.spent(1_000, STANDARD_BUDGET);
    assert_eq!((spent.budget, spent.minute, spent.refused), (STANDARD_BUDGET, 1_800, 1));
    let kinds: Vec<(&str, u32, u32)> =
        spent.kinds.iter().map(|kind| (kind.name.as_str(), kind.count, kind.weight)).collect();
    assert_eq!(kinds, [("orderBooks", 2, 800), ("recentTrades", 1, 600), ("sendTx", 1, 400)]);
    let later = meter.spent(60_041, STANDARD_BUDGET);
    assert_eq!((later.minute, later.refused, later.kinds.len()), (0, 0, 0));
}

#[test]
fn a_strike_closes_the_gate_for_two_seconds_doubling_to_a_minute_unless_retry_after_asks_longer() {
    let mut gate = Gate::default();
    let mut now = 0;
    let mut waits = Vec::new();
    for _ in 0..7 {
        let ticket = gate.open(now).unwrap();
        waits.push(gate.struck(ticket, now, None));
        assert_eq!(gate.open(now + 500).map(|_| ()), Err(waits[waits.len() - 1] - 500), "closed to everything");
        now += waits[waits.len() - 1];
        assert_eq!(gate.cooldown_ms(now), 0);
    }
    assert_eq!(waits, [2_000, 4_000, 8_000, 16_000, 32_000, 60_000, 60_000]);
    for (retry_after, wait) in [(Some(10_000), 10_000), (Some(500), 2_000), (Some(120_000), 120_000)] {
        let mut gate = Gate::default();
        let ticket = gate.open(0).unwrap();
        assert_eq!(gate.struck(ticket, 0, retry_after), wait, "{retry_after:?}");
    }
}

/// The firewall's 405 holds an IP for a minute at once.
#[test]
fn a_firewall_block_closes_the_gate_for_a_minute() {
    let mut gate = Gate::default();
    let ticket = gate.open(0).unwrap();
    assert_eq!(gate.blocked(ticket, 0), MAX_COOLDOWN_MS);
    assert_eq!(gate.cooldown_ms(30_000), 30_000);
}

#[test]
fn a_burst_is_one_strike_and_only_a_success_after_the_last_strike_clears_them() {
    let mut gate = Gate::default();
    let first = gate.open(0).unwrap();
    let second = gate.open(0).unwrap();
    assert_eq!(gate.struck(first, 0, None), 2_000);
    assert_eq!(gate.struck(second, 10, None), 1_990, "sent before the strike, so it does not double the wait");
    let early = gate.open(2_000).unwrap();
    let late = gate.open(2_000).unwrap();
    assert_eq!(gate.struck(late, 2_000, None), 4_000, "the next burst is the second strike");
    gate.passed(early);
    let next = gate.open(6_000).unwrap();
    assert_eq!(gate.struck(next, 6_000, None), 8_000, "the early success predates the strike and clears nothing");
    let ok = gate.open(14_000).unwrap();
    gate.passed(ok);
    let fresh = gate.open(14_000).unwrap();
    assert_eq!(gate.struck(fresh, 14_000, None), 2_000, "a success after the last strike starts over");
}
