use core::fmt;
use std::{
    hash,
    mem::ManuallyDrop,
    ops::Deref,
    sync::{Arc, Mutex, Weak},
};

use derive_where::derive_where;
use hashbrown::{hash_map, Equivalent, HashMap};

// === ArcMap === //

#[derive_where(Clone)]
#[derive_where(Default; S: Default)]
pub struct ArcMap<K, V, S>(Arc<Mutex<ArcMapInner<K, V, S>>>);

type ArcMapInner<K, V, S> = HashMap<(u64, Weak<ArcMapEntry<K, V, S>>), (), S>;

struct ArcMapEntry<K, V, S> {
    map: ArcMap<K, V, S>,
    hash: u64,
    key: K,
    value: V,
}

impl<K: fmt::Debug, V: fmt::Debug, S> fmt::Debug for ArcMap<K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = f.debug_map();

        for (_, obj) in self.0.lock().unwrap().keys() {
            let Some(obj) = obj.upgrade() else { continue };
            map.entry(&obj.key, &obj.value);
        }

        map.finish()
    }
}

impl<K, V, S> ArcMap<K, V, S> {
    pub fn new() -> Self
    where
        S: Default,
    {
        Self::default()
    }

    pub fn with_hasher(hash_builder: S) -> Self {
        Self(Arc::new(Mutex::new(HashMap::with_hasher(hash_builder))))
    }
}

impl<K, V, S: hash::BuildHasher> ArcMap<K, V, S> {
    pub fn get<Q>(&self, key: &Q, default: impl FnOnce() -> (K, V)) -> ArcMapRef<K, V, S>
    where
        Q: ?Sized + hash::Hash + Equivalent<K>,
    {
        let mut map = self.0.lock().unwrap();
        let hash = map.hasher().hash_one(key);

        let mut found_candidate = None;

        let arc = match map
            .raw_entry_mut()
            .from_hash(hash, |(candidate_hash, candidate_obj)| {
                if *candidate_hash != hash {
                    return false;
                }

                let Some(candidate) = candidate_obj.upgrade() else {
                    return false;
                };

                if key.equivalent(&candidate.key) {
                    found_candidate = Some(candidate);
                    true
                } else {
                    false
                }
            }) {
            hash_map::RawEntryMut::Occupied(_) => found_candidate.unwrap(),
            hash_map::RawEntryMut::Vacant(entry) => {
                let (new_key, value) = default();
                debug_assert!(key.equivalent(&new_key));
                let slot = Arc::new(ArcMapEntry {
                    map: self.clone(),
                    hash,
                    key: new_key,
                    value,
                });

                entry
                    .insert_with_hasher(hash, (hash, Arc::downgrade(&slot)), (), |(hash, _)| *hash);
                slot
            }
        };

        ArcMapRef(ManuallyDrop::new(arc))
    }
}

// === ArcMapRef === //

#[derive_where(Clone)]
pub struct ArcMapRef<K, V, S>(ManuallyDrop<Arc<ArcMapEntry<K, V, S>>>);

impl<K: fmt::Debug, V: fmt::Debug, S> fmt::Debug for ArcMapRef<K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArcMapEntry")
            .field("key", &self.0.key)
            .field("value", &self.0.value)
            .finish()
    }
}

impl<K, V, S> ArcMapRef<K, V, S> {
    pub fn map(me: &Self) -> &ArcMap<K, V, S> {
        &me.0.map
    }

    pub fn key(me: &Self) -> &K {
        &me.0.key
    }
}

impl<K, V, S> Deref for ArcMapRef<K, V, S> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.0.value
    }
}

impl<K, V, S> Drop for ArcMapRef<K, V, S> {
    fn drop(&mut self) {
        // If we're the last reference to this entry in the map...
        let arc = unsafe { ManuallyDrop::take(&mut self.0) };
        let arc_ptr = Arc::as_ptr(&arc);
        let Some(entry) = Arc::into_inner(arc) else {
            return;
        };

        // Find the entry's corresponding slot.
        let mut map = entry.map.0.lock().unwrap();

        let hash_map::RawEntryMut::Occupied(entry) = map
            .raw_entry_mut()
            .from_hash(entry.hash, |(_, candidate_obj)| {
                Weak::as_ptr(candidate_obj) == arc_ptr
            })
        else {
            return;
        };

        // If it's not empty, delete it.
        entry.remove();
    }
}
