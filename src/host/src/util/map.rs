use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;

use super::arc_map::{ArcMap, ArcMapRef};

pub type FxHashBuilder = BuildHasherDefault<FxHasher>;

pub type FxHashMap<K, V> = hashbrown::HashMap<K, V, FxHashBuilder>;

pub type FxHashSet<T> = hashbrown::HashSet<T, FxHashBuilder>;

pub type FxArcMap<K, V> = ArcMap<K, V, FxHashBuilder>;

pub type FxArcMapRef<K, V> = ArcMapRef<K, V, FxHashBuilder>;
