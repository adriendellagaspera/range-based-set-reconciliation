// Copyright 2023 Developers of the reconcile project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::borrow::Borrow;
use std::cmp::Ordering;
use std::ops::{Bound, RangeBounds};

use range_cmp::{RangeOrd, RangeOrdering};

use crate::aggregate::Aggregate;

use super::node::Node;
use super::{element, FingerprintTreeMap};

fn range_covers_lower_separator<K: Ord>(start: Bound<&K>, lower_bound: Option<&K>) -> bool {
    match start {
        Bound::Unbounded => true,
        Bound::Included(key) | Bound::Excluded(key) => {
            lower_bound.is_some_and(|lower_bound| key <= lower_bound)
        }
    }
}

fn range_covers_upper_separator<K: Ord>(end: Bound<&K>, upper_bound: Option<&K>) -> bool {
    match end {
        Bound::Unbounded => true,
        Bound::Included(key) | Bound::Excluded(key) => {
            upper_bound.is_some_and(|upper_bound| key >= upper_bound)
        }
    }
}

impl<K: Ord, V> FingerprintTreeMap<K, V> {
    /// Bundled [`Aggregate`] over a range of keys in one `O(log n)` tree walk;
    /// [`Rsos::aggregate`](crate::Rsos::aggregate)'s realization.
    ///
    /// Takes the range by value, as [`range`](Self::range) does.
    pub fn aggregate<R: RangeBounds<K>>(&self, range: R) -> Aggregate {
        fn aux<'a, K: Ord, V, R: RangeBounds<K>>(
            node: &'a Node<K, V>,
            range: &R,
            mut lower_bound: Option<&'a K>,
            upper_bound: Option<&K>,
        ) -> Aggregate {
            crate::counters::record_aggregate_node_visit();
            let lower_bound_included =
                range_covers_lower_separator(range.start_bound(), lower_bound);
            let upper_bound_included = range_covers_upper_separator(range.end_bound(), upper_bound);
            // Both bounds inside the range: the cached subtree aggregate is the answer.
            if lower_bound_included && upper_bound_included {
                crate::counters::record_aggregate_early_exit();
                return node.subtree();
            }
            let mut cum = Aggregate::ZERO;
            // Keep traversal bounded by the node's key count: unlike a `while` cursor, a
            // mutation of the index update cannot turn either scan into a non-terminating loop.
            let mut i = 0;
            for key in &node.keys {
                if key.rcmp(range) != RangeOrdering::Below {
                    break;
                }
                i += 1;
            }
            for index in i..node.keys.len() {
                if node.keys[index].rcmp(range) != RangeOrdering::Inside {
                    break;
                }
                let cur_bound = Some(&node.keys[index]);
                if let Some(children) = node.children.as_ref() {
                    cum += aux(&children[index], range, lower_bound, cur_bound);
                }
                cum += element(node.fingerprint(index));
                lower_bound = cur_bound;
                i = index + 1;
            }
            if let Some(children) = node.children.as_ref() {
                cum += aux(&children[i], range, lower_bound, upper_bound);
            }
            cum
        }
        aux(&self.root, &range, None, None)
    }

    /// Position of `key` in the in-order sequence, or the position it would occupy after
    /// insertion; [`Rsos::rank`](crate::Rsos::rank)'s realization.
    ///
    /// ```
    /// use rsos::FingerprintTreeMap;
    ///
    /// let map: FingerprintTreeMap<i32, &str> = [(10, "a"), (20, "b"), (30, "c")].into_iter().collect();
    ///
    /// // A present key's rank is its in-order index...
    /// assert_eq!(map.rank(&20), 1);
    /// // ...and an absent key still gets the index it would land at if inserted, unlike
    /// // `position`, which is `None` for a key that was never stored.
    /// assert_eq!(map.rank(&15), 1);
    /// assert_eq!(map.rank(&100), map.len());
    /// ```
    pub fn rank<Q>(&self, key: &Q) -> usize
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        fn aux<K: Borrow<Q>, V, Q: Ord + ?Sized>(node: &Node<K, V>, key: &Q) -> usize {
            if let Some(children) = node.children.as_ref() {
                let mut index = 0;
                for i in 0..node.keys.len() {
                    let cmp = node.keys[i].borrow().cmp(key);
                    if cmp == Ordering::Greater {
                        return index + aux(&children[i], key);
                    }
                    index += children[i].subtree_size();
                    if cmp == Ordering::Equal {
                        return index;
                    }
                    index += 1;
                }
                index + aux(children.last().unwrap(), key)
            } else {
                match node.keys.binary_search_by(|probe| probe.borrow().cmp(key)) {
                    Ok(index) => index,
                    Err(index) => index,
                }
            }
        }
        aux(&self.root, key)
    }

    /// Reference to the key at the given in-order position; [`Rsos::select`](crate::Rsos::select)'s
    /// realization.
    ///
    /// # Panics
    ///
    /// If the position is out of bounds.
    ///
    /// ```
    /// use rsos::FingerprintTreeMap;
    ///
    /// let map: FingerprintTreeMap<i32, &str> = [(10, "a"), (20, "b"), (30, "c")].into_iter().collect();
    ///
    /// assert_eq!(map.select(1), &20);
    ///
    /// // `select` and `rank` are inverses over a present key: selecting a key's own rank returns
    /// // that key back.
    /// assert_eq!(map.select(map.rank(&30)), &30);
    /// ```
    #[must_use]
    pub fn select(&self, index: usize) -> &K {
        fn aux<K: Ord, V>(node: &Node<K, V>, mut index: usize) -> &K {
            if let Some(children) = node.children.as_ref() {
                for i in 0..node.keys.len() {
                    if index < children[i].subtree_size() {
                        return aux(&children[i], index);
                    }
                    index -= children[i].subtree_size();
                    if index == 0 {
                        return &node.keys[i];
                    }
                    index -= 1;
                }
                aux(children.last().unwrap(), index)
            } else {
                &node.keys[index]
            }
        }
        aux(&self.root, index)
    }

    /// Number of elements in the tree.
    #[must_use]
    pub fn len(&self) -> usize {
        self.root.subtree_size()
    }

    /// Whether the tree holds no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod separator_coverage_tests {
    use super::{range_covers_lower_separator, range_covers_upper_separator};
    use std::ops::Bound;

    #[test]
    fn lower_separator_coverage_is_inclusive_and_directional() {
        let separator = 10;

        assert!(range_covers_lower_separator::<i32>(
            Bound::Unbounded,
            Some(&separator)
        ));
        assert!(range_covers_lower_separator(
            Bound::Included(&5),
            Some(&separator)
        ));
        assert!(range_covers_lower_separator(
            Bound::Excluded(&10),
            Some(&separator)
        ));
        assert!(!range_covers_lower_separator(
            Bound::Included(&11),
            Some(&separator)
        ));
        assert!(!range_covers_lower_separator(Bound::Included(&5), None));
    }

    #[test]
    fn upper_separator_coverage_is_inclusive_and_directional() {
        let separator = 10;

        assert!(range_covers_upper_separator::<i32>(
            Bound::Unbounded,
            Some(&separator)
        ));
        assert!(range_covers_upper_separator(
            Bound::Included(&15),
            Some(&separator)
        ));
        assert!(range_covers_upper_separator(
            Bound::Excluded(&10),
            Some(&separator)
        ));
        assert!(!range_covers_upper_separator(
            Bound::Included(&9),
            Some(&separator)
        ));
        assert!(!range_covers_upper_separator(Bound::Included(&15), None));
    }
}
