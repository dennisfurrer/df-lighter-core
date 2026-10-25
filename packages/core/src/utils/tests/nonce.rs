//! One key's nonces: counted from the venue's answer, forgotten when they may be wrong.

use super::*;

#[test]
fn nonces_count_up_from_the_venue_s_answer_until_forgotten() {
    let mut nonces = NonceCounter::unknown();
    assert_eq!(nonces.take(), None, "nothing known: ask nextNonce");
    nonces.set(4);
    assert_eq!((nonces.take(), nonces.take(), nonces.next()), (Some(4), Some(5), Some(6)));
    nonces.forget();
    assert_eq!(nonces, NonceCounter::unknown());
    assert_eq!(NonceCounter::at(9).next(), Some(9));
}

#[test]
fn nonces_turned_away_are_handed_out_again_only_when_nothing_followed_them() {
    let mut nonces = NonceCounter::at(7);
    assert_eq!(nonces.take_run(3), Some(7), "a batch of three takes 7, 8 and 9");
    nonces.give_back(7, 3);
    assert_eq!(nonces.take(), Some(7), "nothing after them: 7 again");
    let _later = nonces.take();
    nonces.give_back(7, 1);
    assert_eq!(nonces.next(), None, "8 went out after it, so the count is asked again");
    assert_eq!(nonces.take_run(2), None);
}
