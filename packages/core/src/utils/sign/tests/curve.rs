//! The group law, held to poseidon_crypto's encoding test (`curve_test.go`) and to itself.

use super::*;
use crate::utils::sign::goldilocks::Fp;

#[test]
fn a_point_encodes_as_poseidon_crypto_encodes_it() {
    let point = Point {
        x: Fp5::from_u64s([
            8219099146870311261,
            1751466925979295147,
            7427996218561204331,
            5499363376829590386,
            17146362437196146248,
        ]),
        z: Fp5::from_u64s([
            9697849239028047855,
            5846309906783017685,
            10545493423738651463,
            2054382452661947581,
            7470471124463677860,
        ]),
        u: Fp5::from_u64s([
            2901139745109740356,
            15850005224840060392,
            3464972059371886732,
            15264046134718393739,
            9208307769190416697,
        ]),
        t: Fp5::from_u64s([
            4691886900801030369,
            14793814721360336872,
            14452533794393275351,
            3652664841353278369,
            4894903405053011144,
        ]),
    };
    let expected =
        [11698180777452980608, 17225201015770513568, 2048901991804183462, 12372738397545947475, 13773458998102781339];
    assert_eq!(point.encode().0, expected.map(Fp::new));
}

#[test]
fn multiplication_is_repeated_addition() {
    let scalar = |value: u8| {
        let mut bytes = [0u8; 40];
        bytes[0] = value;
        Scalar::from_le_bytes(&bytes)
    };
    let g = Point::GENERATOR;
    assert_eq!(g.mul(scalar(1)).encode(), g.encode());
    assert_eq!(g.mul(scalar(2)).encode(), g.double().encode());
    assert_eq!(g.mul(scalar(2)).encode(), g.add(g).encode());
    assert_eq!(g.mul(scalar(5)).encode(), g.double().double().add(g).encode());
    assert_eq!(g.mul(scalar(0)).encode(), Fp5::ZERO, "the neutral point encodes as zero");
    assert_eq!(g.add(Point::NEUTRAL).encode(), g.encode());
}
