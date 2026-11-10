//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/UnsignedBitSetTest.java`.

use ig_log::utils::unsigned_bit_set::UnsignedBitSet;

fn to_str(bit_set: &UnsignedBitSet) -> String {
    (-5..=5)
        .map(|i| if bit_set.get(i) { '1' } else { '0' })
        .collect()
}

#[test]
fn init_false() {
    let bit_set = UnsignedBitSet::new();
    assert_eq!("00000000000", to_str(&bit_set));
}

#[test]
fn set_zero() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set(0, true);
    assert_eq!("00000100000", to_str(&bit_set));
}

#[test]
fn set_one() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set(1, true);
    assert_eq!("00000010000", to_str(&bit_set));
}

#[test]
fn set_minus_one() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set(-1, true);
    assert_eq!("00001000000", to_str(&bit_set));
}

#[test]
fn set_several_times() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set(3, true);
    bit_set.set(-3, true);
    bit_set.set(-2, true);
    assert_eq!("00110000100", to_str(&bit_set));
}

#[test]
fn set_positive_range() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set_range(0, 4, true);
    assert_eq!("00000111110", to_str(&bit_set));
}

#[test]
fn set_negative_range() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set_range(-3, -1, true);
    assert_eq!("00111000000", to_str(&bit_set));
}

#[test]
fn set_range() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set_range(-3, 1, true);
    assert_eq!("00111110000", to_str(&bit_set));
}
