use medium_bitset::BitSet;

#[test]
fn set_and_get_basic() {
    let mut bs = BitSet::new(256);
    assert!(!bs.get(0));
    bs.set(0);
    assert!(bs.get(0));
    bs.set(63);
    assert!(bs.get(63));
    bs.set(64);
    assert!(bs.get(64));
    bs.set(255);
    assert!(bs.get(255));
    // out-of-bounds is safe
    bs.set(256);
    assert!(!bs.get(256));
}

#[test]
fn clear_removes_bit() {
    let mut bs = BitSet::new(128);
    bs.set(10);
    assert!(bs.get(10));
    bs.clear(10);
    assert!(!bs.get(10));
    // clearing already-clear bit is fine
    bs.clear(50);
    assert!(!bs.get(50));
}

#[test]
fn count_ones_tracks_population() {
    let mut bs = BitSet::new(128);
    assert_eq!(bs.count_ones(), 0);
    bs.set(0);
    bs.set(1);
    bs.set(100);
    assert_eq!(bs.count_ones(), 3);
    bs.clear(1);
    assert_eq!(bs.count_ones(), 2);
}

#[test]
fn union_combines_both_sets() {
    let mut a = BitSet::new(128);
    let mut b = BitSet::new(128);
    a.set(5);
    a.set(10);
    b.set(10);
    b.set(20);
    let u = a.union(&b);
    assert!(u.get(5));
    assert!(u.get(10));
    assert!(u.get(20));
    assert_eq!(u.count_ones(), 3);
}

#[test]
fn intersection_keeps_common_bits() {
    let mut a = BitSet::new(128);
    let mut b = BitSet::new(128);
    a.set(5);
    a.set(10);
    b.set(10);
    b.set(20);
    let inter = a.intersection(&b);
    assert!(!inter.get(5));
    assert!(inter.get(10));
    assert!(!inter.get(20));
    assert_eq!(inter.count_ones(), 1);
}

#[test]
fn iter_ones_returns_all_set_indices() {
    let mut bs = BitSet::new(256);
    bs.set(0);
    bs.set(3);
    bs.set(63);
    bs.set(64);
    bs.set(200);
    let ones = bs.iter_ones();
    assert_eq!(ones, vec![0, 3, 63, 64, 200]);
}

#[test]
fn toggle_flips_bits() {
    let mut set = BitSet::new(100);
    set.set(5);
    assert!(set.get(5));
    
    set.toggle(5);
    assert!(!set.get(5));
    
    set.toggle(5);
    assert!(set.get(5));
}

#[test]
fn is_empty_works() {
    let empty = BitSet::new(100);
    assert!(empty.is_empty());
    
    let mut set = BitSet::new(100);
    set.set(0);
    assert!(!set.is_empty());
}

#[test]
fn fill_sets_all_bits() {
    let mut set = BitSet::new(100);
    set.fill();
    
    for i in 0..100 {
        assert!(set.get(i), "Bit {} should be set", i);
    }
}

#[test]
fn difference_works() {
    let mut a = BitSet::new(100);
    let mut b = BitSet::new(100);
    
    a.set(1);
    a.set(2);
    a.set(3);
    
    b.set(2);
    b.set(4);
    
    let diff = a.difference(&b);
    assert!(diff.get(1));
    assert!(!diff.get(2));
    assert!(diff.get(3));
    assert!(!diff.get(4));
}

#[test]
fn symmetric_difference_works() {
    let mut a = BitSet::new(100);
    let mut b = BitSet::new(100);
    
    a.set(1);
    a.set(2);
    a.set(3);
    
    b.set(2);
    b.set(4);
    
    let sym_diff = a.symmetric_difference(&b);
    assert!(sym_diff.get(1));
    assert!(!sym_diff.get(2));
    assert!(sym_diff.get(3));
    assert!(sym_diff.get(4));
}

#[test]
fn is_subset_works() {
    let mut small = BitSet::new(100);
    let mut large = BitSet::new(100);
    
    small.set(1);
    small.set(2);
    
    large.set(1);
    large.set(2);
    large.set(3);
    
    assert!(small.is_subset(&large));
    assert!(!large.is_subset(&small));
}

#[test]
fn is_disjoint_works() {
    let mut a = BitSet::new(100);
    let mut b = BitSet::new(100);
    
    a.set(1);
    a.set(2);
    
    b.set(3);
    b.set(4);
    
    assert!(a.is_disjoint(&b));
    
    b.set(2);
    assert!(!a.is_disjoint(&b));
}

#[test]
fn capacity_returns_correct_value() {
    let set = BitSet::new(100);
    assert_eq!(set.capacity(), 100);
    
    let set2 = BitSet::new(1000);
    assert_eq!(set2.capacity(), 1000);
}
