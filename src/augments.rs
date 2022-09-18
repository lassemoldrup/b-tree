use std::ops::{Add, Sub};

use crate::Augment;

impl<K, V> Augment<K, V> for () {
    type Value = ();
    type Output = ();

    fn initial_value() -> Self::Value {}

    fn initial_output() -> Self::Output {}

    fn inserted_sub_tree(_: &K, _: &V, _: &Self::Value) -> Self::Value {}

    fn deleted_sub_tree(_: &K, _: &V, _: &Self::Value) -> Self::Value {}

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

    fn split_root(_: &(K, V), _: &Self::Value, _: &Self::Value) -> Self::Value {}

    fn merge(_: &(K, V), _: &Self::Value, _: &Self::Value) -> Self::Value {}

    fn steal(
        _: &(K, V),
        _: &(K, V),
        _: Option<&Self::Value>,
        _: &Self::Value,
        _: &Self::Value,
    ) -> (Self::Value, Self::Value) {
        ((), ())
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
    }
}

/// Allows for finding the sum of all values associated with smaller (or equal) keys
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

    fn merge((_, parent_value): &(K, V), left: &Self::Value, right: &Self::Value) -> Self::Value {
        &(left + right) + parent_value
    }

    fn steal(
        (_, parent_value): &(K, V),
        (_, victim_value): &(K, V),
        stolen_child: Option<&Self::Value>,
        thief: &Self::Value,
        victim: &Self::Value,
    ) -> (Self::Value, Self::Value) {
        match stolen_child {
            Some(child) => (
                &(thief + parent_value) + child,
                &(victim - victim_value) - child,
            ),
            None => (thief + parent_value, victim - victim_value),
        }
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

    #[test]
    fn simple_summing_delete() {
        let mut tree = BTree::with_augment::<SumAugment>();

        for i in 0..600 {
            tree.insert(i, i);
        }
        tree.delete(&100);
        dbg!(&tree.root);

        assert_eq!(tree.augment_search(&600), (0..600).sum::<i32>() - 100);
    }

    #[test]
    fn simple_summing_delete2() {
        let mut tree = BTree::with_augment::<SumAugment>();

        for i in 0..10 {
            tree.insert(i, i);
        }
        tree.delete(&0);

        assert_eq!(tree.augment_search(&10), (0..10).sum::<i32>());
    }

    #[test]
    fn summing_works_with_delete() {
        let mut tree = BTree::with_augment::<SumAugment>();

        for i in 0..1000 {
            tree.insert(i, i);
        }
        for i in (3000..3700).rev() {
            tree.insert(i, i);
        }

        for i in 500..1000 {
            assert_eq!(tree.delete(&i), Some(i));
        }

        for i in 500..2000 {
            tree.insert(i, i);
        }
        for i in 3500..3700 {
            assert_eq!(tree.delete(&i), Some(i));
        }
        for i in (3500..4000).rev() {
            tree.insert(i, i);
        }

        for i in 1000..2000 {
            assert_eq!(tree.delete(&i), Some(i));
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
