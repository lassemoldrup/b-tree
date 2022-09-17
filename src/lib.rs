use std::mem::{self, MaybeUninit};

const MIN_DEGREE: usize = 4;

struct Node<V> {
    n: usize,
    keys: [MaybeUninit<V>; 2 * MIN_DEGREE - 1],
    children: Vec<Self>,
    leaf: bool,
}

impl<V: Ord> Node<V> {
    const NEW_KEY: MaybeUninit<V> = MaybeUninit::uninit();

    fn new_root() -> Self {
        Self {
            n: 0,
            keys: [Self::NEW_KEY; 2 * MIN_DEGREE - 1],
            children: Vec::with_capacity(2 * MIN_DEGREE),
            leaf: true,
        }
    }

    unsafe fn split(&mut self) -> (V, Self) {
        debug_assert!(self.is_full());

        let median = self.keys[MIN_DEGREE - 1].assume_init_read();

        let mut keys = [Self::NEW_KEY; 2 * MIN_DEGREE - 1];
        self.keys[MIN_DEGREE..].swap_with_slice(&mut keys[..MIN_DEGREE - 1]);

        let children = if self.leaf {
            Vec::with_capacity(2 * MIN_DEGREE)
        } else {
            self.children.split_off(MIN_DEGREE)
        };
        self.n = MIN_DEGREE - 1;

        let new_node = Self {
            n: MIN_DEGREE - 1,
            keys,
            children,
            leaf: self.leaf,
        };

        (median, new_node)
    }

    fn insert_key(&mut self, idx: usize, key: V) {
        debug_assert!(!self.is_full());
        debug_assert!(idx <= self.n);

        for i in (idx + 1..=self.n).rev() {
            self.keys[i] = MaybeUninit::new(unsafe { self.keys[i - 1].assume_init_read() });
        }
        self.keys[idx] = MaybeUninit::new(key);
        self.n += 1;
    }

    fn insert_child(&mut self, idx: usize, child: Self) {
        self.children.insert(idx, child);
    }

    fn find_key_idx(&self, key: &V) -> Result<usize, usize> {
        self.keys[..self.n].binary_search_by_key(&key, |k| unsafe { k.assume_init_ref() })
    }

    /// # Safety
    /// Child at `idx` must be full
    unsafe fn split_child(&mut self, idx: usize) {
        let (median, new_child) = self.children[idx].split();
        self.insert_key(idx, median);
        self.insert_child(idx + 1, new_child);
    }

    fn insert_non_full(&mut self, key: V) {
        debug_assert!(!self.is_full());

        // We ignore duplicates
        let mut idx = match self.find_key_idx(&key) {
            Ok(_) => return,
            Err(i) => i,
        };
        if self.leaf {
            self.insert_key(idx, key);
        } else {
            if self.children[idx].is_full() {
                // Safety: Child is definitely full and `split_child`
                // ensures that `self.keys[idx]` is initialized
                let split_key = unsafe {
                    self.split_child(idx);
                    self.keys[idx].assume_init_ref()
                };

                if &key == split_key {
                    return;
                } else if &key > split_key {
                    idx += 1;
                }
            }
            self.children[idx].insert_non_full(key);
        }
    }

    /// # Safety
    /// `idx` must be in the interval `[0; self.n)`
    unsafe fn remove_key(&mut self, idx: usize) -> V {
        // Extract ownership of the key without using extra work
        let key = self.keys[idx].assume_init_read();
        self.n -= 1;
        for i in idx..self.n {
            self.keys[i] = MaybeUninit::new(self.keys[i + 1].assume_init_read());
        }
        key
    }

    fn delete_max(&mut self) -> V {
        if self.leaf {
            return unsafe { self.remove_key(self.n - 1) };
        }

        if self.children[self.n].is_min() {
            unsafe {
                self.make_space(self.n);
            }
        }

        self.children[self.n].delete_max()
    }

    fn delete_min(&mut self) -> V {
        if self.leaf {
            return unsafe { self.remove_key(0) };
        }

        if self.children[0].is_min() {
            unsafe {
                self.make_space(0);
            }
        }

        self.children[0].delete_min()
    }

    /// # Safety
    /// Child `idx` and `idx + 1` must exist and have have mininum degree
    unsafe fn merge_children(&mut self, idx: usize) {
        let parent_key = self.remove_key(idx);

        let mut right_child = self.children.remove(idx + 1);
        let left_child = &mut self.children[idx];

        left_child.keys[MIN_DEGREE - 1] = MaybeUninit::new(parent_key);
        for i in 0..MIN_DEGREE - 1 {
            let key = right_child.keys[i].assume_init_read();
            left_child.keys[i + MIN_DEGREE] = MaybeUninit::new(key);
        }
        left_child.n = 2 * MIN_DEGREE - 1;

        if !left_child.leaf {
            left_child.children.append(&mut right_child.children);
        }
    }

    /// # Safety
    /// `idx` must be in the range `[0; self.n)`
    unsafe fn delete_own(&mut self, idx: usize) {
        if self.leaf {
            self.remove_key(idx);
        } else if !self.children[idx].is_min() {
            self.keys[idx] = MaybeUninit::new(self.children[idx].delete_max());
        } else if !self.children[idx + 1].is_min() {
            self.keys[idx] = MaybeUninit::new(self.children[idx + 1].delete_min());
        } else {
            self.merge_children(idx);
            self.children[idx].delete_own(MIN_DEGREE - 1);
        }
    }

    /// # Safety
    /// Child with index `idx` must exist and not be full
    unsafe fn make_space(&mut self, mut idx: usize) -> usize {
        if idx > 0 && !self.children[idx - 1].is_min() {
            // Steal a key from the left sibling (through parent)
            self.children[idx].insert_key(0, self.keys[idx - 1].assume_init_read());

            let sibling = &mut self.children[idx - 1];
            self.keys[idx - 1] = MaybeUninit::new(sibling.remove_key(sibling.n - 1));

            if !sibling.leaf {
                let last_child = sibling.children.pop().unwrap();
                self.children[idx].children.insert(0, last_child);
            }
        } else if idx < self.n && !self.children[idx + 1].is_min() {
            // Steal a key from the right sibling (through parent)
            let child_n = self.children[idx].n;
            self.children[idx].insert_key(child_n, self.keys[idx].assume_init_read());

            let sibling = &mut self.children[idx + 1];
            self.keys[idx] = MaybeUninit::new(sibling.remove_key(0));

            if !sibling.leaf {
                let first_child = sibling.children.remove(0);
                self.children[idx].children.push(first_child);
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

    fn delete_in_decendant(&mut self, mut idx: usize, key: &V) {
        if self.leaf {
            return;
        }

        if self.children[idx].is_min() {
            idx = unsafe { self.make_space(idx) };
        }

        self.children[idx].delete(key);
    }

    fn delete(&mut self, key: &V) {
        match self.find_key_idx(key) {
            Ok(idx) => unsafe { self.delete_own(idx) },
            Err(idx) => self.delete_in_decendant(idx, key),
        }
    }

    fn search(&self, key: &V) -> bool {
        let idx = match self.find_key_idx(&key) {
            Ok(_) => return true,
            Err(i) => i,
        };
        if self.leaf {
            false
        } else {
            self.children[idx].search(key)
        }
    }

    fn is_min(&self) -> bool {
        self.n <= MIN_DEGREE - 1
    }

    fn is_full(&self) -> bool {
        self.n == 2 * MIN_DEGREE - 1
    }
}

/// BTree based on the "Introduction to Algorithms" book
pub struct BTree<V> {
    root: Node<V>,
}

impl<V: Ord> BTree<V> {
    pub fn new() -> Self {
        BTree {
            root: Node::new_root(),
        }
    }

    pub fn insert(&mut self, key: V) {
        if self.root.is_full() {
            let (root_key, child) = unsafe { self.root.split() };
            let mut old_root = Node::new_root();
            mem::swap(&mut self.root, &mut old_root);

            self.root.keys[0] = MaybeUninit::new(root_key);
            self.root.children.push(old_root);
            self.root.children.push(child);
            self.root.leaf = false;
            self.root.n = 1;
        }

        self.root.insert_non_full(key);
    }

    pub fn delete(&mut self, key: &V) {
        self.root.delete(key);
    }

    pub fn search(&self, key: &V) -> bool {
        self.root.search(key)
    }
}

#[cfg(test)]
mod tests {
    use crate::BTree;

    fn setup_tree() -> BTree<i32> {
        let mut tree = BTree::new();

        assert!(!tree.search(&100));

        for i in 0..1000 {
            tree.insert(i);
            tree.insert(i);
        }
        for i in (1000..2000).rev() {
            tree.insert(i);
        }
        for i in 2000..3000 {
            tree.insert(i);
        }
        for i in (3000..4000).rev() {
            tree.insert(i);
        }

        tree
    }

    #[test]
    fn insert_and_search_works() {
        let tree = setup_tree();

        for i in 0..4000 {
            assert!(tree.search(&i));
        }
        for i in 4000..5000 {
            assert!(!tree.search(&i));
        }
        assert!(!tree.search(&-1));
    }

    #[test]
    fn deletion_works() {
        let mut tree = setup_tree();

        for i in 0..4000 {
            if i % 5 == 0 || i % 11 == 0 {
                tree.delete(&i);
            }
        }

        for i in 0..4000 {
            if i % 5 == 0 || i % 11 == 0 {
                assert!(!tree.search(&i));
            } else {
                assert!(tree.search(&i));
            }
        }
    }
}
