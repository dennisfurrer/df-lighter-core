//! Client order indexes: the clock's millisecond, one past the last when that is later, in range
//! always.

use super::*;
use crate::utils::ids::WIDTH_48;

const NOW: u64 = 1_790_000_000_000;

#[test]
fn an_index_is_the_millisecond_and_a_burst_in_one_still_differs() {
    let indices = ClientIndices::new();
    let burst: Vec<u64> = (0..3).map(|_| indices.next(NOW).get()).collect();
    assert_eq!(burst, [NOW, NOW + 1, NOW + 2]);
    assert_eq!(indices.next(NOW + 1).get(), NOW + 3, "a clock that falls behind the count is passed");
    assert_eq!(indices.next(NOW + 10).get(), NOW + 10);
}

#[test]
fn an_index_seen_elsewhere_is_counted_past() {
    let indices = ClientIndices::new();
    indices.observe(ClientOrderIndex::new(NOW + 50).unwrap());
    assert_eq!(indices.next(NOW).get(), NOW + 51);
    indices.observe(ClientOrderIndex::of::<5>());
    assert_eq!(indices.next(NOW).get(), NOW + 52, "a lower one changes nothing");
}

#[test]
fn indexes_stay_within_1_and_2_pow_48_less_1() {
    let indices = ClientIndices::new();
    assert_eq!(indices.next(0).get(), 1, "never 0, which names no index");
    indices.observe(ClientOrderIndex::new(WIDTH_48).unwrap());
    assert_eq!(indices.next(NOW).get(), 1, "past the last it counts on from 1");
    assert_eq!(indices.next(u64::MAX).get(), 2, "a clock past the range is not used");
    assert_eq!(indices.next(NOW).get(), NOW);
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn one_set_of_indexes_is_shared_safely() {
    static SHARED: ClientIndices = ClientIndices::new();
    let handed: Vec<u64> = std::thread::scope(|scope| {
        let threads: Vec<_> =
            (0..4).map(|_| scope.spawn(|| (0..100).map(|_| SHARED.next(NOW).get()).collect::<Vec<_>>())).collect();
        threads.into_iter().flat_map(|thread| thread.join().unwrap()).collect()
    });
    let mut unique = handed.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), handed.len());
}
