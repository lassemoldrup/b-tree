use std::cmp::Ordering;
use std::fmt::{self, Debug, Formatter};
use std::mem::{self, MaybeUninit};

pub mod augments;

const MIN_DEGREE: usize = 6;

pub trait Augment<K, V> {
    type Value;
    type Output;

    fn initial_value() -> Self::Value;

    fn initial_output() -> Self::Output;

    fn inserted_sub_tree(key: &K, value: &V, old: &Self::Value) -> Self::Value;

    fn deleted_sub_tree(key: &K, value: &V, old: &Self::Value) -> Self::Value;

    fn split<'a>(
        left_keys: &[(K, V)],
        right_keys: &[(K, V)],
        median: &(K, V),
        left_children: impl Iterator<Item = &'a Self::Value>,
        right_children: impl Iterator<Item = &'a Self::Value>,
        old: &Self::Value,
    ) -> (Self::Value, Self::Value)
    where
        Self::Value: 'a;

    fn split_root(root_pair: &(K, V), left: &Self::Value, right: &Self::Value) -> Self::Value;

    fn merge(parent_pair: &(K, V), left: &Self::Value, right: &Self::Value) -> Self::Value;

    /// Should return (Thief, Victim)
    fn steal(
        parent_pair: &(K, V),
        victim_pair: &(K, V),
        stolen_child: Option<&Self::Value>,
        thief: &Self::Value,
        victim: &Self::Value,
    ) -> (Self::Value, Self::Value);

    fn visit<'a>(
        found: bool,
        idx: usize,
        keys: &[(K, V)],
        children: impl Iterator<Item = &'a Self::Value>,
        value: &Self::Value,
        acc: Self::Output,
    ) -> Self::Output
    where
        Self::Value: 'a;
}

struct Node<K, V, A: Augment<K, V>> {
    n: usize,
    keys: [MaybeUninit<(K, V)>; 2 * MIN_DEGREE - 1],
    children: Vec<Self>,
    aug_val: A::Value,
}

impl<K: Ord, V, A: Augment<K, V>> Node<K, V, A> {
    const NEW_KEY: MaybeUninit<(K, V)> = MaybeUninit::uninit();

    fn new_root() -> Self {
        Self {
            n: 0,
            keys: [Self::NEW_KEY; 2 * MIN_DEGREE - 1],
            children: Vec::with_capacity(2 * MIN_DEGREE),
            aug_val: A::initial_value(),
        }
    }

    unsafe fn split(&mut self) -> ((K, V), Self) {
        debug_assert!(self.is_full());

        let median = self.keys[MIN_DEGREE - 1].assume_init_read();

        let mut keys = [Self::NEW_KEY; 2 * MIN_DEGREE - 1];
        self.keys[MIN_DEGREE..].swap_with_slice(&mut keys[..MIN_DEGREE - 1]);

        let children = if self.is_leaf() {
            Vec::with_capacity(2 * MIN_DEGREE)
        } else {
            self.children.split_off(MIN_DEGREE)
        };
        self.n = MIN_DEGREE - 1;

        let augment;
        (self.aug_val, augment) = A::split(
            mem::transmute(&self.keys[..MIN_DEGREE - 1]),
            mem::transmute(&keys[..MIN_DEGREE - 1]),
            &median,
            self.children.iter().map(|n| &n.aug_val),
            children.iter().map(|n| &n.aug_val),
            &self.aug_val,
        );

        let new_node = Self {
            n: MIN_DEGREE - 1,
            keys,
            children,
            aug_val: augment,
        };

        (median, new_node)
    }

    fn insert_pair(&mut self, idx: usize, pair: (K, V)) {
        debug_assert!(!self.is_full());
        debug_assert!(idx <= self.n);

        for i in (idx + 1..=self.n).rev() {
            self.keys[i] = MaybeUninit::new(unsafe { self.keys[i - 1].assume_init_read() });
        }
        self.keys[idx] = MaybeUninit::new(pair);
        self.n += 1;
    }

    fn insert_child(&mut self, idx: usize, child: Self) {
        self.children.insert(idx, child);
    }

    fn find_key_idx(&self, key: &K) -> Result<usize, usize> {
        self.keys[..self.n].binary_search_by_key(&key, |k| unsafe { &k.assume_init_ref().0 })
    }

    /// # Safety
    /// Child at `idx` must be full
    unsafe fn split_child(&mut self, idx: usize) {
        let (median, new_child) = self.children[idx].split();
        self.insert_pair(idx, median);
        self.insert_child(idx + 1, new_child);
    }

    fn insert_non_full(&mut self, key: K, value: V) -> Result<(), (K, V)> {
        debug_assert!(!self.is_full());

        // We ignore duplicates
        let mut idx = match self.find_key_idx(&key) {
            Ok(_) => return Err((key, value)),
            Err(i) => i,
        };

        if self.is_leaf() {
            self.aug_val = A::inserted_sub_tree(&key, &value, &self.aug_val);
            self.insert_pair(idx, (key, value));
            Ok(())
        } else {
            if self.children[idx].is_full() {
                // Safety: Child is definitely full and `split_child`
                // ensures that `self.keys[idx]` is initialized
                let split_key = unsafe {
                    self.split_child(idx);
                    &self.keys[idx].assume_init_ref().0
                };

                match key.cmp(split_key) {
                    Ordering::Equal => return Err((key, value)),
                    Ordering::Greater => idx += 1,
                    Ordering::Less => {}
                }
            }

            self.aug_val = A::inserted_sub_tree(&key, &value, &self.aug_val);
            // If we end up not inserting the key, because it is a duplicate, undo the augment update
            self.children[idx]
                .insert_non_full(key, value)
                .map_err(|(k, v)| {
                    self.aug_val = A::deleted_sub_tree(&k, &v, &self.aug_val);
                    (k, v)
                })
        }
    }

    /// # Safety
    /// `idx` must be in the interval `[0; self.n)`
    unsafe fn remove_pair(&mut self, idx: usize) -> (K, V) {
        // Extract ownership of the key without using extra work
        let pair = self.keys[idx].assume_init_read();
        self.n -= 1;
        for i in idx..self.n {
            self.keys[i] = MaybeUninit::new(self.keys[i + 1].assume_init_read());
        }
        pair
    }

    /// # Safety
    /// Must not be empty
    unsafe fn delete_max(&mut self) -> (K, V) {
        if self.is_leaf() {
            let (key, value) = self.remove_pair(self.n - 1);
            self.aug_val = A::deleted_sub_tree(&key, &value, &self.aug_val);
            return (key, value);
        }

        if self.children[self.n].is_min() {
            self.make_space(self.n);
        }

        let (key, value) = self.children[self.n].delete_max();
        self.aug_val = A::deleted_sub_tree(&key, &value, &self.aug_val);
        (key, value)
    }

    /// # Safety
    /// Must not be empty
    unsafe fn delete_min(&mut self) -> (K, V) {
        if self.is_leaf() {
            let (key, value) = self.remove_pair(0);
            self.aug_val = A::deleted_sub_tree(&key, &value, &self.aug_val);
            return (key, value);
        }

        if self.children[0].is_min() {
            self.make_space(0);
        }

        let (key, value) = self.children[0].delete_min();
        self.aug_val = A::deleted_sub_tree(&key, &value, &self.aug_val);
        (key, value)
    }

    /// # Safety
    /// Child `idx` and `idx + 1` must exist and have have mininum degree
    unsafe fn merge_children(&mut self, idx: usize) {
        let parent_pair = self.remove_pair(idx);

        let mut right_child = self.children.remove(idx + 1);
        let left_child = &mut self.children[idx];

        left_child.aug_val = A::merge(&parent_pair, &left_child.aug_val, &right_child.aug_val);

        left_child.keys[MIN_DEGREE - 1] = MaybeUninit::new(parent_pair);
        for i in 0..MIN_DEGREE - 1 {
            let key = right_child.keys[i].assume_init_read();
            left_child.keys[i + MIN_DEGREE] = MaybeUninit::new(key);
        }
        left_child.n = 2 * MIN_DEGREE - 1;

        if !left_child.is_leaf() {
            left_child.children.append(&mut right_child.children);
        }
    }

    /// # Safety
    /// `idx` must be in the range `[0; self.n)`
    unsafe fn delete_own(&mut self, key: &K, idx: usize) -> V {
        let value = if self.is_leaf() {
            self.remove_pair(idx).1
        } else if !self.children[idx].is_min() {
            let (_, value) = self.keys[idx].assume_init_read();
            self.keys[idx] = MaybeUninit::new(self.children[idx].delete_max());
            value
        } else if !self.children[idx + 1].is_min() {
            let (_, value) = self.keys[idx].assume_init_read();
            self.keys[idx] = MaybeUninit::new(self.children[idx + 1].delete_min());
            value
        } else {
            self.merge_children(idx);
            self.children[idx].delete_own(key, MIN_DEGREE - 1)
        };

        self.aug_val = A::deleted_sub_tree(key, &value, &self.aug_val);
        value
    }

    /// # Safety
    /// Child with index `idx` must exist and not be full
    unsafe fn make_space(&mut self, mut idx: usize) -> usize {
        if idx > 0 && !self.children[idx - 1].is_min() {
            // Steal a key from the left sibling (through parent)
            let (victim_slice, thief_slice) = self.children.split_at_mut(idx);
            let thief = &mut thief_slice[0];
            let victim = &mut victim_slice[idx - 1];

            let parent_pair = self.keys[idx - 1].assume_init_read();
            let sibling_pair = victim.remove_pair(victim.n - 1);

            let stolen_child = if victim.is_leaf() {
                None
            } else {
                Some(victim.children.pop().unwrap())
            };

            (thief.aug_val, victim.aug_val) = A::steal(
                &parent_pair,
                &sibling_pair,
                stolen_child.as_ref().map(|c| &c.aug_val),
                &thief.aug_val,
                &victim.aug_val,
            );

            self.keys[idx - 1] = MaybeUninit::new(sibling_pair);
            thief.insert_pair(0, parent_pair);

            if let Some(child) = stolen_child {
                thief.children.insert(0, child);
            }
        } else if idx < self.n && !self.children[idx + 1].is_min() {
            // Steal a key from the right sibling (through parent)
            let (thief_slice, victim_slice) = self.children.split_at_mut(idx + 1);
            let thief = &mut thief_slice[idx];
            let victim = &mut victim_slice[0];

            let parent_pair = self.keys[idx].assume_init_read();
            let sibling_pair = victim.remove_pair(0);

            let stolen_child = if victim.is_leaf() {
                None
            } else {
                Some(victim.children.remove(0))
            };

            (thief.aug_val, victim.aug_val) = A::steal(
                &parent_pair,
                &sibling_pair,
                stolen_child.as_ref().map(|c| &c.aug_val),
                &thief.aug_val,
                &victim.aug_val,
            );

            self.keys[idx] = MaybeUninit::new(sibling_pair);
            thief.insert_pair(thief.n, parent_pair);

            if let Some(child) = stolen_child {
                thief.children.push(child);
            }
        } else if idx > 0 {
            // We can merge with the left sibling
            idx -= 1;
            self.merge_children(idx);
        } else {
            // Merge with right sibling
            self.merge_children(idx);
        }

        idx
    }

    fn delete_in_decendant(&mut self, mut idx: usize, key: &K) -> Option<V> {
        if self.is_leaf() {
            return None;
        }

        if self.children[idx].is_min() {
            idx = unsafe { self.make_space(idx) };
        }

        self.children[idx].delete(key).map(|v| {
            self.aug_val = A::deleted_sub_tree(key, &v, &self.aug_val);
            v
        })
    }

    fn delete(&mut self, key: &K) -> Option<V> {
        match self.find_key_idx(key) {
            Ok(idx) => unsafe { Some(self.delete_own(key, idx)) },
            Err(idx) => self.delete_in_decendant(idx, key),
        }
    }

    fn search(&self, key: &K, mut acc: A::Output) -> (Option<&V>, A::Output) {
        let (idx, found) = match self.find_key_idx(key) {
            Ok(i) => (i, true),
            Err(i) => (i, false),
        };

        acc = A::visit(
            found,
            idx,
            unsafe { mem::transmute(&self.keys[..self.n]) },
            self.children.iter().map(|n| &n.aug_val),
            &self.aug_val,
            acc,
        );

        if found {
            (Some(unsafe { &self.keys[idx].assume_init_ref().1 }), acc)
        } else if self.is_leaf() {
            (None, acc)
        } else {
            self.children[idx].search(key, acc)
        }
    }

    fn is_min(&self) -> bool {
        self.n < MIN_DEGREE
    }

    fn is_full(&self) -> bool {
        self.n == 2 * MIN_DEGREE - 1
    }

    fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

impl<K, V, A> Debug for Node<K, V, A>
where
    A: Augment<K, V>,
    K: Debug,
    V: Debug,
    A::Value: Debug,
{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Node")
            .field(
                "keys",
                &format!("[{}]", unsafe {
                    mem::transmute::<_, &[(K, V)]>(&self.keys[..self.n])
                        .iter()
                        .map(|(k, v)| format!("({k:?}, {v:?}), "))
                        .collect::<String>()
                        .strip_suffix(", ")
                        .unwrap()
                }),
            )
            .field("children", &self.children)
            .field("aug_val", &self.aug_val)
            .finish()
    }
}

/// BTree based on the "Introduction to Algorithms" book
pub struct BTree<K, V, A: Augment<K, V> = ()> {
    root: Node<K, V, A>,
}

impl<K: Ord, V> BTree<K, V> {
    pub fn new() -> Self {
        Self {
            root: Node::new_root(),
        }
    }

    pub fn with_augment<T: Augment<K, V>>() -> BTree<K, V, T> {
        BTree {
            root: Node::new_root(),
        }
    }
}

impl<K: Ord, V, A: Augment<K, V>> BTree<K, V, A> {
    pub fn insert(&mut self, key: K, value: V) -> bool {
        if self.root.is_full() {
            let (root_pair, child) = unsafe { self.root.split() };

            let mut old_root = Node::new_root();
            mem::swap(&mut self.root, &mut old_root);

            self.root.aug_val = A::split_root(&root_pair, &old_root.aug_val, &child.aug_val);
            self.root.keys[0] = MaybeUninit::new(root_pair);
            self.root.children.push(old_root);
            self.root.children.push(child);
            self.root.n = 1;
        }

        self.root.insert_non_full(key, value).is_ok()
    }

    pub fn delete(&mut self, key: &K) -> Option<V> {
        let res = self.root.delete(key);
        if self.root.children.len() == 1 {
            self.root = self.root.children.pop().unwrap();
        }
        res
    }

    pub fn search(&self, key: &K) -> Option<&V> {
        self.root.search(key, A::initial_output()).0
    }

    pub fn augment_search(&self, key: &K) -> A::Output {
        self.root.search(key, A::initial_output()).1
    }
}

impl<K: Ord, V, A: Augment<K, V>> Default for BTree<K, V, A> {
    fn default() -> Self {
        Self {
            root: Node::new_root(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::BTree;

    fn setup_tree_set() -> BTree<i32, (), ()> {
        let mut tree = BTree::new();

        assert!(tree.search(&100).is_none());

        for i in 0..1000 {
            assert!(tree.insert(i, ()));
            assert!(!tree.insert(i, ()));
        }
        for i in (3000..4000).rev() {
            tree.insert(i, ());
        }
        for i in 2000..3000 {
            tree.insert(i, ());
        }
        for i in (1000..2000).rev() {
            tree.insert(i, ());
        }

        tree
    }

    #[test]
    fn insert_and_search_works_set() {
        let tree = setup_tree_set();

        for i in 0..4000 {
            assert!(tree.search(&i).is_some());
        }
        for i in 4000..5000 {
            assert!(tree.search(&i).is_none());
        }
        assert!(tree.search(&-1).is_none());
    }

    #[test]
    fn deletion_works() {
        let mut tree = setup_tree_set();

        for i in 0..4000 {
            if i % 5 == 0 || i % 11 == 0 {
                assert!(tree.delete(&i).is_some());
            }
        }

        for i in 1000..3000 {
            if i % 5 == 0 || i % 11 == 0 {
                assert!(tree.delete(&i).is_none());
            }
        }

        for i in 0..4000 {
            if i % 5 == 0 || i % 11 == 0 {
                assert!(tree.search(&i).is_none());
            } else {
                assert!(tree.search(&i).is_some());
            }
        }
    }

    #[test]
    fn associated_values_work() {
        let mut tree = BTree::new();

        for i in 0..4000 {
            tree.insert(i, i * 2);
        }

        for i in 0..4000 {
            assert_eq!(tree.search(&i), Some(&(i * 2)));
        }
    }
}
