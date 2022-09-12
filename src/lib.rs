use std::fmt::Debug;
use std::mem::{self, MaybeUninit};

const MIN_DEGREE: usize = 4;

struct Node<V> {
    n: usize,
    keys: [MaybeUninit<V>; 2 * MIN_DEGREE - 1],
    children: [MaybeUninit<Box<Self>>; 2 * MIN_DEGREE],
    leaf: bool,
}

impl<V: Ord + Copy + Debug> Node<V> {
    const KEY: MaybeUninit<V> = MaybeUninit::uninit();
    const CHILD: MaybeUninit<Box<Self>> = MaybeUninit::uninit();

    const fn new_root() -> Self {
        Self {
            n: 0,
            keys: [Self::KEY; 2 * MIN_DEGREE - 1],
            children: [Self::CHILD; 2 * MIN_DEGREE],
            leaf: true,
        }
    }

    /// # Safety
    /// UB if Node is not full
    unsafe fn split(&mut self) -> (V, Self) {
        let mut median = Self::KEY;
        mem::swap(&mut median, &mut self.keys[MIN_DEGREE - 1]);
        let median = median.assume_init();

        let mut keys = [Self::KEY; 2 * MIN_DEGREE - 1];
        let mut children = [Self::CHILD; 2 * MIN_DEGREE];

        self.keys[MIN_DEGREE..].swap_with_slice(&mut keys[..MIN_DEGREE - 1]);
        if !self.leaf {
            self.children[MIN_DEGREE..].swap_with_slice(&mut children[..MIN_DEGREE]);
        }
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
        self.keys.copy_within(idx..self.n, idx + 1);
        self.keys[idx] = MaybeUninit::new(key);
        self.n += 1;
    }

    fn insert_child(&mut self, idx: usize, child: Self) {
        for i in (idx + 1..=self.n).rev() {
            self.children.swap(i, i - 1);
        }
        self.children[idx] = MaybeUninit::new(Box::new(child));
    }

    fn find_key_idx(&self, key: &V) -> Result<usize, usize> {
        // Safety: The first `self.n` keys are initialized
        self.keys[..self.n].binary_search_by_key(key, |k| unsafe { k.assume_init() })
    }

    /// # Safety
    /// UB if Node is full or `idx` is out of bounds
    unsafe fn split_child(&mut self, idx: usize) {
        let child = self.children[idx].assume_init_mut();
        let (median, new_child) = child.split();
        self.insert_key(idx, median);
        self.insert_child(idx + 1, new_child);
    }

    fn insert_non_full(&mut self, key: V) {
        // We ignore duplicates
        let mut idx = match self.find_key_idx(&key) {
            Ok(_) => return,
            Err(i) => i,
        };
        if self.leaf {
            self.insert_key(idx, key);
        } else {
            // Safety: `idx` <= `self.n`, so that child will be initialized
            let child = unsafe { self.children[idx].assume_init_ref() };
            if child.is_full() {
                // Safety: `self` is assumed to be non-full and `idx` is still in bounds
                let split_key = unsafe {
                    self.split_child(idx);
                    self.keys[idx].assume_init()
                };

                if key == split_key {
                    return;
                } else if key > split_key {
                    idx += 1;
                }
            }
            let child = unsafe { self.children[idx].assume_init_mut() };
            child.insert_non_full(key);
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
            let child = unsafe { self.children[idx].assume_init_ref() };
            child.search(key)
        }
    }

    fn is_full(&self) -> bool {
        self.n == 2 * MIN_DEGREE - 1
    }
}

pub struct BTree<V> {
    root: Node<V>,
}

impl<V: Ord + Copy + Debug> BTree<V> {
    pub const fn new() -> Self {
        BTree {
            root: Node::new_root(),
        }
    }

    pub fn insert(&mut self, key: V) {
        if self.root.is_full() {
            // Safety: Root is full
            let (root_key, child) = unsafe { self.root.split() };
            let mut old_root = Node::new_root();
            mem::swap(&mut self.root, &mut old_root);

            self.root.keys[0] = MaybeUninit::new(root_key);
            self.root.children[0] = MaybeUninit::new(Box::new(old_root));
            self.root.children[1] = MaybeUninit::new(Box::new(child));
            self.root.leaf = false;
            self.root.n = 1;
        }

        self.root.insert_non_full(key);
    }

    pub fn search(&self, key: &V) -> bool {
        println!("Searching for {:?}", key);
        self.root.search(key)
    }
}

#[cfg(test)]
mod tests {
    use crate::BTree;

    #[test]
    fn works() {
        let mut tree = BTree::new();

        assert!(!tree.search(&100));

        for i in 0..2000 {
            tree.insert(i);
        }
        for i in (2000..4000).rev() {
            tree.insert(i);
        }

        for i in 0..4000 {
            assert!(tree.search(&i));
        }

        assert!(!tree.search(&-1));
        assert!(!tree.search(&5000));
    }
}
