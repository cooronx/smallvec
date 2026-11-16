use {
    heapless::{
        Deque,
        Vec as HeaplessVec
    },
    smallvec::SmallVec,
    std::{
        cell::Cell,
        cmp::Ordering,
        rc::Rc
    }
};

/// Counts how many times it has been dropped, to catch leaks and double drops.
struct Tracked(Rc<Cell<usize>>);

impl Drop for Tracked {
    fn drop(&mut self) { self.0.set(self.0.get() + 1); }
}

fn tracked<const N: usize>(count: usize, drops: &Rc<Cell<usize>>) -> HeaplessVec<Tracked, N> {
    (0..count).map(|_| Tracked(drops.clone())).collect()
}

fn deque<T, const N: usize>(items: impl IntoIterator<Item = T>) -> Deque<T, N> {
    let mut deque = Deque::new();
    deque.extend(items);
    deque
}

fn wrapped_deque() -> Deque<i32, 4> {
    let mut deque = deque([0, 1, 2, 3]);
    deque.pop_front();
    deque.push_back(4).unwrap();
    assert!(!deque.as_slices().1.is_empty());
    deque
}

#[test]
fn from_heapless_stays_inline_or_spills() {
    let small: SmallVec<i32, 4> = HeaplessVec::<i32, 8, u8>::from_iter([1, 2, 3]).into();
    assert!(!small.spilled());
    assert_eq!(&small[..], &[1, 2, 3]);

    let small: SmallVec<i32, 2> = HeaplessVec::<i32, 8>::from_iter([1, 2, 3, 4, 5]).into();
    assert!(small.spilled());
    assert_eq!(&small[..], &[1, 2, 3, 4, 5]);

    let small: SmallVec<i32, 4> = wrapped_deque().into();
    assert!(!small.spilled());
    assert_eq!(&small[..], &[1, 2, 3, 4]);
    let small: SmallVec<i32, 2> = wrapped_deque().into();
    assert!(small.spilled());
    assert_eq!(&small[..], &[1, 2, 3, 4]);
}

#[test]
fn try_from_smallvec_fits_or_returns_original() {
    // Inline source, exactly filling the vector.
    let vector =
        HeaplessVec::<i32, 4, u8>::try_from(SmallVec::<i32, 4>::from([1, 2, 3, 4])).unwrap();
    assert_eq!(&vector[..], &[1, 2, 3, 4]);
    // Spilled source.
    let vector =
        HeaplessVec::<i32, 8>::try_from(SmallVec::<i32, 2>::from([1, 2, 3, 4, 5])).unwrap();
    assert_eq!(&vector[..], &[1, 2, 3, 4, 5]);

    // One element too many: the original comes back untouched.
    let error =
        HeaplessVec::<i32, 3>::try_from(SmallVec::<i32, 4>::from([1, 2, 3, 4])).unwrap_err();
    assert!(!error.spilled());
    assert_eq!(&error[..], &[1, 2, 3, 4]);
    let error =
        HeaplessVec::<i32, 4>::try_from(SmallVec::<i32, 2>::from([1, 2, 3, 4, 5])).unwrap_err();
    assert!(error.spilled());
    assert_eq!(&error[..], &[1, 2, 3, 4, 5]);

    let deque = Deque::<i32, 4>::try_from(SmallVec::<i32, 4>::from([1, 2, 3, 4])).unwrap();
    assert!(deque.iter().copied().eq([1, 2, 3, 4]));
    let deque = Deque::<i32, 8>::try_from(SmallVec::<i32, 2>::from([1, 2, 3, 4, 5])).unwrap();
    assert!(deque.iter().copied().eq([1, 2, 3, 4, 5]));
    let error = Deque::<i32, 3>::try_from(SmallVec::<i32, 4>::from([1, 2, 3, 4])).unwrap_err();
    assert!(!error.spilled());
    assert_eq!(&error[..], &[1, 2, 3, 4]);
    let error = Deque::<i32, 4>::try_from(SmallVec::<i32, 2>::from([1, 2, 3, 4, 5])).unwrap_err();
    assert!(error.spilled());
    assert_eq!(&error[..], &[1, 2, 3, 4, 5]);
}

#[test]
fn conversions_drop_each_element_exactly_once() {
    // `From`, both inline (3 <= 4) and spilled (6 > 4).
    for count in [3, 6] {
        let drops = Rc::new(Cell::new(0));
        let small: SmallVec<Tracked, 4> = tracked::<8>(count, &drops).into();
        assert_eq!(small.spilled(), count > 4);
        assert_eq!(drops.get(), 0, "conversion must not drop elements");
        drop(small);
        assert_eq!(drops.get(), count);
    }

    // `TryFrom` success, both inline and spilled.
    for count in [3, 6] {
        let drops = Rc::new(Cell::new(0));
        let small: SmallVec<Tracked, 4> = tracked::<8>(count, &drops).into();
        let vector = HeaplessVec::<Tracked, 8>::try_from(small).ok().unwrap();
        assert_eq!(drops.get(), 0, "conversion must not drop elements");
        drop(vector);
        assert_eq!(drops.get(), count);
    }

    // `TryFrom` failure hands every element back.
    let drops = Rc::new(Cell::new(0));
    let small: SmallVec<Tracked, 2> = tracked::<8>(5, &drops).into();
    let error = HeaplessVec::<Tracked, 3>::try_from(small).err().unwrap();
    assert_eq!(drops.get(), 0, "failed conversion must not drop elements");
    drop(error);
    assert_eq!(drops.get(), 5);

    for count in [3, 6] {
        let drops = Rc::new(Cell::new(0));
        let small: SmallVec<Tracked, 4> = deque::<_, 8>(tracked::<8>(count, &drops)).into();
        assert_eq!(small.spilled(), count > 4);
        assert_eq!(drops.get(), 0, "conversion must not drop elements");
        drop(small);
        assert_eq!(drops.get(), count);

        let drops = Rc::new(Cell::new(0));
        let small: SmallVec<Tracked, 4> = tracked::<8>(count, &drops).into_iter().collect();
        let deque = Deque::<Tracked, 8>::try_from(small).ok().unwrap();
        assert_eq!(drops.get(), 0, "conversion must not drop elements");
        drop(deque);
        assert_eq!(drops.get(), count);

        let drops = Rc::new(Cell::new(0));
        let small: SmallVec<Tracked, 4> = tracked::<8>(count, &drops).into_iter().collect();
        let error = Deque::<Tracked, 2>::try_from(small).err().unwrap();
        assert_eq!(drops.get(), 0, "failed conversion must not drop elements");
        drop(error);
        assert_eq!(drops.get(), count);
    }
}

#[test]
fn zero_sized_items() {
    let vector: HeaplessVec<(), 8> = std::iter::repeat_n((), 5).collect();
    let small: SmallVec<(), 2> = vector.into();
    assert_eq!(small.len(), 5);

    assert!(HeaplessVec::<(), 4>::try_from(small.clone()).is_err());
    assert_eq!(HeaplessVec::<(), 5>::try_from(small).unwrap().len(), 5);

    let small: SmallVec<(), 2> = deque::<_, 8>([(); 5]).into();
    assert_eq!(small.len(), 5);
    assert!(Deque::<(), 4>::try_from(small.clone()).is_err());
    assert_eq!(Deque::<(), 5>::try_from(small).unwrap().len(), 5);
}

#[test]
fn eq_across_capacities_and_item_types() {
    let small = SmallVec::<i32, 2>::from([1, 2, 3]);
    let vector = HeaplessVec::<i32, 8>::from_iter([1, 2, 3]);
    assert!(small == vector);
    assert!(vector == small);
    assert!(small != HeaplessVec::<i32, 8>::from_iter([1, 2, 4]));
    assert!(vector != SmallVec::<i32, 2>::from([1, 2]));

    // `Item: PartialEq<Other>` with different types.
    let small: SmallVec<String, 2> = SmallVec::from(["a".to_string(), "b".to_string()]);
    let vector = HeaplessVec::<&str, 4>::from_iter(["a", "b"]);
    assert!(small == vector);
    assert!(vector == small);

    let small = SmallVec::<i32, 2>::from([1, 2, 3, 4]);
    let wrapped = wrapped_deque();
    assert!(small == wrapped);
    assert!(wrapped == small);
    assert!(small != deque::<_, 8>([1, 2, 3, 5]));
    assert!(wrapped != SmallVec::<i32, 2>::from([1, 2, 3]));
    let small = SmallVec::<String, 2>::from(["a".to_string(), "b".to_string()]);
    let deque = deque::<_, 4>(["a", "b"]);
    assert!(small == deque);
    assert!(deque == small);
}

#[test]
fn ordering_in_both_directions() {
    let small = SmallVec::<i32, 2>::from([1, 2, 3]);
    let vector = HeaplessVec::<i32, 8>::from_iter([1, 2, 4]);
    assert!(small < vector);
    assert!(vector > small);
    assert_eq!(small.partial_cmp(&vector), Some(Ordering::Less));
    assert_eq!(vector.partial_cmp(&small), Some(Ordering::Greater));

    let small = SmallVec::<i32, 2>::from([1, 2, 3, 3]);
    let deque = wrapped_deque();
    assert!(small < deque);
    assert!(deque > small);
    assert_eq!(small.partial_cmp(&deque), Some(Ordering::Less));
    assert_eq!(deque.partial_cmp(&small), Some(Ordering::Greater));
}
