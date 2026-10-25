//! The published Keccak-256 vectors, input fed in pieces, and the borrowed permutation checked
//! against one written out here.

use super::*;
use crate::utils::hex;

const ROUND_CONSTANTS: [u64; 24] = [
    0x0000_0000_0000_0001,
    0x0000_0000_0000_8082,
    0x8000_0000_0000_808a,
    0x8000_0000_8000_8000,
    0x0000_0000_0000_808b,
    0x0000_0000_8000_0001,
    0x8000_0000_8000_8081,
    0x8000_0000_0000_8009,
    0x0000_0000_0000_008a,
    0x0000_0000_0000_0088,
    0x0000_0000_8000_8009,
    0x0000_0000_8000_000a,
    0x0000_0000_8000_808b,
    0x8000_0000_0000_008b,
    0x8000_0000_0000_8089,
    0x8000_0000_0000_8003,
    0x8000_0000_0000_8002,
    0x8000_0000_0000_0080,
    0x0000_0000_0000_800a,
    0x8000_0000_8000_000a,
    0x8000_0000_8000_8081,
    0x8000_0000_0000_8080,
    0x0000_0000_8000_0001,
    0x8000_0000_8000_8008,
];

const RHO: [u32; 25] = [
    0, 1, 62, 28, 27, //
    36, 44, 6, 55, 20, //
    3, 10, 43, 25, 39, //
    41, 45, 15, 21, 8, //
    18, 2, 61, 56, 14,
];

/// Pi permutation, computed rather than transcribed so no hand-typed table can be wrong.
const PI: [usize; 25] = {
    let mut moves = [0usize; 25];
    let mut x = 0;
    while x < 5 {
        let mut y = 0;
        while y < 5 {
            moves[x + 5 * y] = y + 5 * ((2 * x + 3 * y) % 5);
            y += 1;
        }
        x += 1;
    }
    moves
};

/// The permutation written out, to check the borrowed one against.
fn permute_portable(state: &mut [u64; 25]) {
    for &round_constant in &ROUND_CONSTANTS {
        let mut column = [0u64; 5];
        for (x, parity) in column.iter_mut().enumerate() {
            *parity = state[x] ^ state[x + 5] ^ state[x + 10] ^ state[x + 15] ^ state[x + 20];
        }
        let mut spread = [0u64; 5];
        for (x, value) in spread.iter_mut().enumerate() {
            *value = column[(x + 4) % 5] ^ column[(x + 1) % 5].rotate_left(1);
        }
        for (index, lane) in state.iter_mut().enumerate() {
            *lane ^= spread[index % 5];
        }

        let mut moved = [0u64; 25];
        for (index, &lane) in state.iter().enumerate() {
            moved[PI[index]] = lane.rotate_left(RHO[index]);
        }

        for (y, row) in moved.chunks_exact(5).enumerate() {
            let out = &mut state[y * 5..y * 5 + 5];
            out[0] = row[0] ^ (!row[1] & row[2]);
            out[1] = row[1] ^ (!row[2] & row[3]);
            out[2] = row[2] ^ (!row[3] & row[4]);
            out[3] = row[3] ^ (!row[4] & row[0]);
            out[4] = row[4] ^ (!row[0] & row[1]);
        }

        state[0] ^= round_constant;
    }
}

/// The published vectors, and any input the same however it is fed in: across the 136-byte
/// block, in pieces, or all at once.
#[test]
fn the_published_vectors_hold_however_the_input_is_split() {
    for (input, digest) in [
        (&b""[..], "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"),
        (&b"abc"[..], "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"),
        (
            &b"The quick brown fox jumps over the lazy dog"[..],
            "4d741b6f1eb29cb2a9b9911c82f56fa8d73b04959d3d9d222895df6c0b28aa15",
        ),
    ] {
        assert_eq!(hex::encode(&keccak256(input)), digest);
        let mut sponge = Keccak256::default();
        for byte in input {
            sponge.update(&[*byte]);
        }
        assert_eq!(hex::encode(&sponge.finalize()), digest, "a byte at a time");
    }
    let whole = b"the quick brown fox jumps over the lazy dog, repeatedly, \
                  for long enough to cross a 136 byte block boundary and \
                  then some more so the tail is short";
    assert!(whole.len() > RATE, "this needs to span two blocks");
    for split in [0, 1, 7, 8, RATE - 1, RATE, RATE + 1, whole.len()] {
        let pieced = Keccak256::new().chain(&whole[..split]).chain(&whole[split..]).finalize();
        assert_eq!(pieced, keccak256(whole), "split at {split}");
    }
    assert_eq!(format!("{:?}", Keccak256::new()), "Keccak256", "the state is never shown");
}

#[test]
fn the_borrowed_permutation_is_the_one_this_build_would_have_written() {
    let mut mine = [0u64; 25];
    let mut theirs = [0u64; 25];
    for (at, (a, b)) in mine.iter_mut().zip(theirs.iter_mut()).enumerate() {
        let lane = 0x0123_4567_89ab_cdefu64.wrapping_mul(at as u64 + 1).rotate_left(at as u32);
        *a = lane;
        *b = lane;
    }
    for round in 0..64 {
        permute_portable(&mut mine);
        permute(&mut theirs);
        assert_eq!(mine, theirs, "they parted at round {round}");
    }
    assert_ne!(mine, [0u64; 25], "and the state actually moved");
}

fn sixty_four(seed: u64) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    let mut x = seed ^ 0x9e37_79b9_7f4a_7c15;
    for byte in &mut bytes {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *byte = x.to_le_bytes()[0];
    }
    bytes
}

/// Two digests taken in one pass are the two taken one at a time, the same input twice
/// included, and the zeros' digest is the published one.
#[test]
fn two_digests_taken_together_are_the_two_taken_apart() {
    for seed in 0..20_000u64 {
        let a = sixty_four(seed);
        let b = sixty_four(seed.wrapping_mul(31).wrapping_add(7));
        let (left, right) = keccak256_of_two(&a, &b);
        assert_eq!(left, keccak256(&a), "left of pair {seed}");
        assert_eq!(right, keccak256(&b), "right of pair {seed}");
    }
    let same = sixty_four(1);
    assert_eq!(keccak256_of_two(&same, &same), (keccak256(&same), keccak256(&same)));
    let (zeros, _) = keccak256_of_two(&[0u8; 64], &[1u8; 64]);
    assert_eq!(hex::encode(&zeros), "ad3228b676f7d3cd4284a5443f17f1962b36e491b30a40b2405849e597ba5fb5");
}

/// What the ARMv8.2 instructions are worth over the permutation written out here.
#[test]
#[ignore = "a measurement, not an assertion; run with --ignored --nocapture"]
fn how_fast_the_permutation_is_either_way() {
    use std::time::Instant;

    const MANY: u32 = 2_000_000;
    let start = [0x0123_4567_89ab_cdefu64; 25];

    let mut ours = start;
    let began = Instant::now();
    for _ in 0..MANY {
        permute_portable(&mut ours);
    }
    let portable = began.elapsed();

    let mut theirs = start;
    let began = Instant::now();
    for _ in 0..MANY {
        permute(&mut theirs);
    }
    let dispatched = began.elapsed();

    assert_eq!(ours, theirs, "the two permutations disagree");

    let each = |s: std::time::Duration| s.as_nanos() / u128::from(MANY);
    let ratio = each(portable) * 100 / each(dispatched).max(1);
    println!("\n  this build's permutation : {:>4} ns", each(portable));
    println!("  keccak crate, dispatched : {:>4} ns (hardware: {})", each(dispatched), hardware());
    println!("  speedup                  : {}.{:02}x\n", ratio / 100, ratio % 100);
}

/// What the second lane is worth: a digest one at a time against two in one pass.
#[test]
#[ignore = "a measurement, not an assertion; run with --ignored --nocapture"]
fn how_much_a_second_lane_is_worth() {
    use std::time::Instant;

    const MANY: u64 = 2_000_000;
    let inputs: Vec<[u8; 64]> = (0..256).map(sixty_four).collect();
    let mut sink = 0u8;

    let began = Instant::now();
    for i in 0..MANY {
        sink ^= keccak256(&inputs[(i as usize) & 255])[0];
    }
    let one = began.elapsed();

    let began = Instant::now();
    for i in 0..MANY / 2 {
        let (l, r) = keccak256_of_two(&inputs[(i as usize * 2) & 255], &inputs[(i as usize * 2 + 1) & 255]);
        sink ^= l[0] ^ r[0];
    }
    let two = began.elapsed();

    let per = |d: std::time::Duration| d.as_nanos() / u128::from(MANY);
    let ratio = one.as_nanos() * 100 / two.as_nanos().max(1);
    println!("\n  one at a time   : {:>4} ns a digest", per(one));
    println!("  two at a time   : {:>4} ns a digest", per(two));
    println!("  speedup         : {:>4}.{:02}x  (sink {sink})\n", ratio / 100, ratio % 100);
}
