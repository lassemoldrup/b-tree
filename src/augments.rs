use std::ops::{Add, Sub};

use crate::Augment;

impl<K, V> Augment<K, V> for () {
    type Value = ();
    type Output = ();

    fn initial_value() -> Self::Value {
        ()
    }

    fn initial_output() -> Self::Output {
        ()
    }

    fn inserted_sub_tree(_: &K, _: &V, _: &Self::Value) -> Self::Value {
        ()
    }

    fn deleted_sub_tree(_: &K, _: &V, _: &Self::Value) -> Self::Value {
        ()
    }

    fn split<'a>(
        _: &[(K, V)],
        _: &[(K, V)],
        _: &(K, V),
        _: impl Iterator<Item = &'a Self::Value>,
        _: impl Iterator<Item = &'a Self::Value>,
        _: &Self::Value,
    ) -> (Self::Value, Self::Value) {
        ((), ())
    }

    fn split_root(_: &(K, V), _: &Self::Value, _: &Self::Value) -> Self::Value {
        ()
    }

    fn visit<'a>(
        _: bool,
        _: usize,
        _: &[(K, V)],
        _: impl Iterator<Item = &'a Self::Value>,
        _: &Self::Value,
        _: Self::Output,
    ) -> Self::Output
    where
        Self::Value: 'a,
    {
        ()
    }
}

/// Allows for finding the sum of all values associated with smaller (or equal) keys
#[derive(Debug)]
pub struct SumAugment;

impl<K, V: Default> Augment<K, V> for SumAugment
where
    for<'a> &'a V: Add<Output = V> + Sub<Output = V>,
{
    type Value = V;
    type Output = V;

    fn initial_value() -> Self::Value {
        V::default()
    }

    fn initial_output() -> Self::Output {
        V::default()
    }

    fn inserted_sub_tree(_: &K, value: &V, old: &Self::Value) -> Self::Value {
        old + value
    }

    fn deleted_sub_tree(_: &K, value: &V, old: &Self::Value) -> Self::Value {
        old - value
    }

    fn split<'a>(
        left_keys: &[(K, V)],
        _: &[(K, V)],
        (_, median_value): &(K, V),
        left_children: impl Iterator<Item = &'a Self::Value>,
        _: impl Iterator<Item = &'a Self::Value>,
        old: &Self::Value,
    ) -> (Self::Value, Self::Value)
    where
        Self::Value: 'a,
    {
        let mut left = V::default();
        for (_, value) in left_keys.iter() {
            left = &left + value;
        }
        for aug_val in left_children {
            left = &left + aug_val;
        }

        let right = &(old - median_value) - &left;
        (left, right)
    }

    fn split_root(
        (_, root_value): &(K, V),
        left: &Self::Value,
        right: &Self::Value,
    ) -> Self::Value {
        &(root_value + left) + right
    }

    fn visit<'a>(
        found: bool,
        idx: usize,
        keys: &[(K, V)],
        children: impl Iterator<Item = &'a Self::Value>,
        _: &Self::Value,
        mut acc: Self::Output,
    ) -> Self::Output
    where
        Self::Value: 'a,
    {
        for (_, value) in &keys[..idx] {
            acc = &acc + value;
        }

        let num_children = if found {
            acc = &acc + &keys[idx].1;
            idx + 1
        } else {
            idx
        };

        for aug_val in children.take(num_children) {
            acc = &acc + aug_val;
        }

        acc
    }
}

#[cfg(test)]
mod tests {
    use crate::augments::SumAugment;
    use crate::BTree;

    #[test]
    fn summing_works_no_delete() {
        let mut tree = BTree::with_augment::<SumAugment>();

        assert_eq!(tree.augment_search(&100), 0);

        for i in 0..500 {
            tree.insert(i, i);
        }
        for i in (3000..3500).rev() {
            tree.insert(i, i);
        }
        for i in 500..1000 {
            tree.insert(i, i);
        }
        for i in (3500..4000).rev() {
            tree.insert(i, i);
        }

        assert_eq!(tree.augment_search(&2000), (0..1000).sum());
        assert_eq!(tree.augment_search(&750), (0..=750).sum());
        assert_eq!(
            tree.augment_search(&3400),
            (0..1000).sum::<i32>() + (3000..=3400).sum::<i32>()
        );
        assert_eq!(
            tree.augment_search(&5000),
            (0..1000).sum::<i32>() + (3000..4000).sum::<i32>()
        );
    }
}
