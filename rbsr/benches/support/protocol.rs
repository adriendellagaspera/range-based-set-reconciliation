use std::cell::Cell;
use std::ops::RangeBounds;

use rand::rngs::StdRng;
use rbsr::{
    initial_ranges, protocol_round_with_policy, EnumerationRange, RangeAggregate, RefinementPolicy,
};
use rsos::{Aggregate, Rsos};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Queries {
    pub aggregate: usize,
    pub rank: usize,
    pub select: usize,
}

impl std::ops::Add for Queries {
    type Output = Queries;

    fn add(self, other: Queries) -> Queries {
        Queries {
            aggregate: self.aggregate + other.aggregate,
            rank: self.rank + other.rank,
            select: self.select + other.select,
        }
    }
}

pub struct Counting<'a, S> {
    inner: &'a S,
    aggregate: Cell<usize>,
    rank: Cell<usize>,
    select: Cell<usize>,
}

impl<'a, S> Counting<'a, S> {
    pub fn new(inner: &'a S) -> Self {
        Self {
            inner,
            aggregate: Cell::new(0),
            rank: Cell::new(0),
            select: Cell::new(0),
        }
    }

    pub fn queries(&self) -> Queries {
        Queries {
            aggregate: self.aggregate.get(),
            rank: self.rank.get(),
            select: self.select.get(),
        }
    }
}

impl<K, S: Rsos<K>> Rsos<K> for Counting<'_, S> {
    type Value = S::Value;

    fn size(&self) -> usize {
        self.inner.size()
    }

    fn aggregate<R: RangeBounds<K>>(&self, range: R) -> Aggregate {
        self.aggregate.set(self.aggregate.get() + 1);
        self.inner.aggregate(range)
    }

    fn rank(&self, key: &K) -> usize {
        self.rank.set(self.rank.get() + 1);
        self.inner.rank(key)
    }

    fn select(&self, rank: usize) -> &K {
        self.select.set(self.select.get() + 1);
        self.inner.select(rank)
    }

    fn enumerate<'a, R: RangeBounds<K> + 'a>(
        &'a self,
        range: R,
    ) -> impl Iterator<Item = (&'a K, &'a Self::Value)> + 'a
    where
        K: Ord + 'a,
        Self::Value: 'a,
    {
        self.inner.enumerate(range)
    }

    fn insert(&mut self, _key: K, _value: Self::Value) -> Option<Self::Value> {
        unreachable!("the reconciliation driver never mutates the store")
    }

    fn delete(&mut self, _key: &K) -> Option<Self::Value> {
        unreachable!("the reconciliation driver never mutates the store")
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Cost {
    pub messages: usize,
    pub ranges: usize,
    pub enumerations: usize,
    pub enumerated_elements: usize,
    pub queries: Queries,
}

pub fn reconcile<S: Rsos<u64>>(
    a: &S,
    b: &S,
    policy: &dyn RefinementPolicy,
    rng: &mut StdRng,
) -> Cost {
    let mut cost = Cost::default();
    let mut active: Vec<RangeAggregate<u64>> = initial_ranges(a);
    let mut responder_is_b = true;

    while !active.is_empty() {
        cost.messages += 1;
        cost.ranges += active.len();

        let responder = if responder_is_b { b } else { a };
        let mut children = Vec::new();
        let mut enumerations: Vec<EnumerationRange<u64>> = Vec::new();

        protocol_round_with_policy(
            responder,
            policy,
            active,
            &mut children,
            &mut enumerations,
            rng,
        );

        cost.enumerations += enumerations.len();
        for range in enumerations {
            cost.enumerated_elements += responder.enumerate(range).count();
        }

        active = children;
        responder_is_b = !responder_is_b;
        assert!(
            cost.messages < 100_000,
            "reconciliation failed to converge — refinement did not shrink"
        );
    }

    cost
}
