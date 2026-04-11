use std::convert::TryInto;
use std::hash::{BuildHasher, Hash, Hasher};

use ahash::RandomState;
use nohash::IntMap;
use parking_lot::{
    MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard,
};

const SHARD_BITS: usize = 12;
const SHARDS: usize = 1 << SHARD_BITS;
const SHARD_SHIFT: usize = 64 - SHARD_BITS;

pub struct StateMap<V, S = RandomState> {
    hasher: S,
    buckets: Box<[RwLock<IntMap<u64, V>>; SHARDS]>,
}

impl<V, S: Default> Default for StateMap<V, S> {
    fn default() -> Self {
        Self {
            hasher: Default::default(),
            buckets: std::iter::repeat_with(|| RwLock::new(IntMap::default()))
                .take(SHARDS)
                .collect::<Box<_>>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
        }
    }
}

impl<V> StateMap<V> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<V, S: BuildHasher> StateMap<V, S> {
    pub fn index(&self, key: &impl Hash) -> u64 {
        let mut hasher = self.hasher.build_hasher();
        key.hash(&mut hasher);
        hasher.finish()
    }

    fn bucket(&self, raw: u64) -> &RwLock<IntMap<u64, V>> {
        &self.buckets[((raw >> SHARD_SHIFT) as usize) & (SHARDS - 1)]
    }

    pub fn get_raw(&self, raw: u64) -> Option<MappedRwLockReadGuard<'_, V>> {
        RwLockReadGuard::try_map(self.bucket(raw).read(), |shard| shard.get(&raw)).ok()
    }

    pub fn get(&self, key: &impl Hash) -> Option<MappedRwLockReadGuard<'_, V>> {
        self.get_raw(self.index(key))
    }

    pub fn get_raw_mut(&self, raw: u64) -> Option<MappedRwLockWriteGuard<'_, V>> {
        RwLockWriteGuard::try_map(self.bucket(raw).write(), |shard| shard.get_mut(&raw)).ok()
    }

    pub fn get_raw_or_insert_with(
        &self,
        raw: u64,
        f: impl FnOnce() -> V,
    ) -> MappedRwLockWriteGuard<'_, V> {
        RwLockWriteGuard::map(self.bucket(raw).write(), |shard| {
            shard.entry(raw).or_insert_with(f)
        })
    }

    pub fn get_or_insert_with(
        &self,
        key: &impl Hash,
        f: impl FnOnce() -> V,
    ) -> MappedRwLockWriteGuard<'_, V> {
        self.get_raw_or_insert_with(self.index(key), f)
    }

    pub fn map_values<T>(self, f: impl Fn(V) -> T) -> StateMap<T, S> {
        StateMap {
            hasher: self.hasher,
            buckets: self
                .buckets
                .into_iter()
                .map(|shard| {
                    RwLock::new(
                        shard
                            .into_inner()
                            .into_iter()
                            .map(|(raw, value)| (raw, f(value)))
                            .collect(),
                    )
                })
                .collect::<Box<_>>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
        }
    }
}
