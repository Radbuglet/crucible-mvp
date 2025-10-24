use rustc_hash::FxBuildHasher;

pub type FxHashMap<K, V> = hashbrown::HashMap<K, V, FxBuildHasher>;
pub type FxHashSet<T> = hashbrown::HashSet<T, FxBuildHasher>;
