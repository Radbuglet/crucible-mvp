use std::{
    any::Any,
    borrow::Borrow,
    fmt,
    hash::{self, BuildHasher as _, Hash},
    mem,
    ops::Deref,
    sync::{
        Arc, OnceLock, RwLock, Weak,
        atomic::{AtomicU64, Ordering::*},
    },
};

use derive_where::derive_where;
use hashbrown::hash_map;
use rustc_hash::FxBuildHasher;

use crate::{
    impl_tuples,
    utils::{FxHashMap, MappedArc},
};

// === AssetManager === //

#[derive(Debug, Clone, Default)]
pub struct AssetManager(Arc<AssetManagerInner>);

#[derive(Debug, Default)]
struct AssetManagerInner {
    asset_map: RwLock<FxHashMap<AssetKeyErased, Weak<AssetEntry>>>,
    id_gen: AtomicU64,
}

#[derive(Debug)]
struct AssetKeyErased {
    hash: u64,
    id: u64,
    func: usize,
    value: Box<dyn AssetKeyOwned>,
}

type AssetEntryValue<T> = OnceLock<AssetEntryValueInner<T>>;

struct AssetEntryValueInner<T> {
    _keep_alive: Vec<AssetKeepAlive>,
    value: T,
}

struct AssetEntry<T: ?Sized = dyn Any + Send + Sync> {
    manager: Weak<AssetManagerInner>,
    hash: u64,
    id: u64,
    value: T,
}

impl<T: ?Sized> Drop for AssetEntry<T> {
    fn drop(&mut self) {
        let Some(manager) = self.manager.upgrade() else {
            return;
        };

        let mut map = manager.asset_map.write().unwrap();

        let hash_map::RawEntryMut::Occupied(entry) = map
            .raw_entry_mut()
            .from_hash(self.hash, |v| v.id == self.id)
        else {
            return;
        };

        // N.B. `AssetKeyErased` contains user a controlled destructor which may recursively drop
        // other `AssetEntry` arcs. We avoid a dead-lock by dropping the map before the entry.
        let kv = entry.remove_entry();
        drop(map);
        drop(manager);
        drop(kv);
    }
}

impl AssetManager {
    pub fn new() -> Self {
        Self::default()
    }

    #[expect(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.0.asset_map.read().unwrap().len()
    }

    pub fn fetch_untracked<C, K, O>(
        &self,
        key: K,
        loader: fn(&mut AssetManagerTracked, C, K) -> O,
    ) -> Option<Asset<O>>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        self.fetch_entry::<K>(&key, loader as usize, Self::hash_key(&key, loader as usize))
            .and_then(|entry| {
                MappedArc::try_new(entry, |entry| {
                    Result::<_, ()>::Ok(
                        &entry
                            .value
                            .downcast_ref::<AssetEntryValue<O>>()
                            .unwrap()
                            .get()
                            .ok_or(())?
                            .value,
                    )
                })
                .ok()
            })
            .map(Asset)
    }

    pub fn load_untracked<C, K, O>(
        &self,
        context: C,
        key: K,
        loader: fn(&mut AssetManagerTracked, C, K) -> O,
    ) -> Asset<O>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        let entry = self.load_entry::<K, O>(&key, loader as usize);

        Asset(MappedArc::new(entry, |entry| {
            &entry
                .value
                .downcast_ref::<AssetEntryValue<O>>()
                .unwrap()
                .get_or_init(|| {
                    let mut tracked = AssetManagerTracked::new(self.clone());
                    let out = loader(&mut tracked, context, key);

                    AssetEntryValueInner {
                        _keep_alive: tracked.into_keep_alive(),
                        value: out,
                    }
                })
                .value
        }))
    }

    fn hash_key<K: AssetKey>(key: &K, func: usize) -> u64 {
        FxBuildHasher.hash_one(&(func, AssetKeyHashAdapter(&key)))
    }

    fn check_key<K: AssetKey>(hash: u64, func: usize, key: &K, candidate: &AssetKeyErased) -> bool {
        candidate.hash == hash
            && candidate.func == func
            && candidate
                .value
                .as_any()
                .downcast_ref::<K::Owned>()
                .is_some_and(|v| key.matches_key(v))
    }

    fn fetch_entry<K: AssetKey>(&self, key: &K, func: usize, hash: u64) -> Option<Arc<AssetEntry>> {
        self.0
            .asset_map
            .read()
            .unwrap()
            .raw_entry()
            .from_hash(hash, |candidate| {
                Self::check_key(hash, func, key, candidate)
            })
            .and_then(|(_k, v)| v.upgrade())
    }

    fn load_entry<K, O>(&self, key: &K, func: usize) -> Arc<AssetEntry>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        let hash = Self::hash_key(key, func);

        // Fast path: reuse an existing entry
        if let Some(entry) = self.fetch_entry(key, func, hash) {
            return entry;
        }

        // Slow path: insert a new entry
        let mut assets = self.0.asset_map.write().unwrap();

        match assets.raw_entry_mut().from_hash(hash, |candidate| {
            Self::check_key(hash, func, key, candidate)
        }) {
            hash_map::RawEntryMut::Occupied(mut entry) => {
                if let Some(value) = entry.get().upgrade() {
                    return value;
                }

                // We can't reuse the key's existing ID since a dropped `Asset` may be in the
                // process of removing the original asset instance and we want to ensure that it
                // doesn't delete this revived entry.
                let id = self.0.id_gen.fetch_add(1, Relaxed);

                entry.key_mut().id = id;

                let value = Arc::new(AssetEntry::<AssetEntryValue<O>> {
                    manager: Arc::downgrade(&self.0),
                    hash,
                    id,
                    value: OnceLock::new(),
                });
                let value = value as Arc<AssetEntry>;

                entry.insert(Arc::downgrade(&value));

                value
            }
            hash_map::RawEntryMut::Vacant(entry) => {
                let id = self.0.id_gen.fetch_add(1, Relaxed);
                let value = Arc::new(AssetEntry::<AssetEntryValue<O>> {
                    manager: Arc::downgrade(&self.0),
                    hash,
                    id,
                    value: OnceLock::new(),
                });
                let value = value as Arc<AssetEntry>;

                entry.insert_with_hasher(
                    hash,
                    AssetKeyErased {
                        hash,
                        id,
                        func,
                        value: Box::new(key.to_owned_key()),
                    },
                    Arc::downgrade(&value),
                    |k| k.hash,
                );

                value
            }
        }
    }
}

// === AssetLoader === //

pub trait AssetLoader: Sized {
    fn manager(&self) -> &AssetManager;

    fn push_keep_alive(&mut self, keep_alive: &AssetKeepAlive);

    fn load<C, K, O>(
        &mut self,
        context: C,
        key: K,
        loader: fn(&mut AssetManagerTracked, C, K) -> O,
    ) -> Asset<O>
    where
        K: AssetKey,
        O: 'static + Send + Sync;

    fn load_alias<C, K, O>(
        &mut self,
        context: C,
        key: K,
        loader: fn(&mut AssetManagerTracked, C, K) -> Asset<O>,
    ) -> Asset<O>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        Asset::map(self.load(context, key, loader), |v| &**v)
    }
}

#[derive(Debug)]
pub struct AssetManagerTracked {
    manager: AssetManager,
    keep_alive: Vec<AssetKeepAlive>,
}

impl AssetManagerTracked {
    pub fn new(manager: AssetManager) -> Self {
        Self {
            manager,
            keep_alive: Vec::new(),
        }
    }

    pub fn keep_alive(&self) -> &[AssetKeepAlive] {
        &self.keep_alive
    }

    pub fn into_keep_alive(self) -> Vec<AssetKeepAlive> {
        self.keep_alive
    }
}

impl AssetLoader for AssetManagerTracked {
    fn manager(&self) -> &AssetManager {
        &self.manager
    }

    fn push_keep_alive(&mut self, keep_alive: &AssetKeepAlive) {
        self.keep_alive.push(keep_alive.clone());
    }

    fn load<C, K, O>(
        &mut self,
        context: C,
        key: K,
        loader: fn(&mut AssetManagerTracked, C, K) -> O,
    ) -> Asset<O>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        let asset = self.manager().load_untracked(context, key, loader);
        self.push_keep_alive(Asset::keep_alive(&asset));
        asset
    }
}

// === Asset === //

#[derive_where(Clone)]
pub struct Asset<T: ?Sized>(MappedArc<AssetEntry, T>);

impl<T: ?Sized + fmt::Debug> fmt::Debug for Asset<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Asset")
            .field("id", &MappedArc::original(&self.0).id)
            .field("value", &<MappedArc<_, _> as Deref>::deref(&self.0))
            .finish()
    }
}

impl<T: ?Sized> hash::Hash for Asset<T> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(MappedArc::original(&self.0)).hash(state);
    }
}

impl<T: ?Sized> Eq for Asset<T> {}

impl<T: ?Sized> PartialEq for Asset<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(MappedArc::original(&self.0), MappedArc::original(&other.0))
    }
}

impl<T: ?Sized> Deref for Asset<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ?Sized> Borrow<AssetKeepAlive> for Asset<T> {
    fn borrow(&self) -> &AssetKeepAlive {
        Self::keep_alive(self)
    }
}

impl<T: ?Sized> Asset<T> {
    pub fn new_untracked(value: T) -> Self
    where
        T: 'static + Sized + Send + Sync,
    {
        Asset(MappedArc::new(
            Arc::new(AssetEntry::<AssetEntryValue<T>> {
                manager: Weak::default(),
                hash: 0,
                id: 0,
                value: OnceLock::from(AssetEntryValueInner {
                    _keep_alive: Vec::new(),
                    value,
                }),
            }),
            |v| {
                &v.value
                    .downcast_ref::<AssetEntryValue<T>>()
                    .unwrap()
                    .get()
                    .unwrap()
                    .value
            },
        ))
    }

    pub fn try_map<V: ?Sized, E>(
        me: Self,
        map: impl FnOnce(&T) -> Result<&V, E>,
    ) -> Result<Asset<V>, (Asset<T>, E)> {
        match MappedArc::try_map(me.0, map) {
            Ok(v) => Ok(Asset(v)),
            Err((v, e)) => Err((Asset(v), e)),
        }
    }

    pub fn map<V: ?Sized>(me: Self, map: impl FnOnce(&T) -> &V) -> Asset<V> {
        Asset(MappedArc::map(me.0, map))
    }

    pub fn keep_alive(me: &Self) -> &AssetKeepAlive {
        unsafe { &*(MappedArc::original(&me.0) as *const Arc<AssetEntry> as *const AssetKeepAlive) }
    }

    pub fn into_keep_alive(me: Self) -> AssetKeepAlive {
        AssetKeepAlive(MappedArc::into_original(me.0))
    }
}

#[derive(Clone)]
#[repr(transparent)]
pub struct AssetKeepAlive(Arc<AssetEntry>);

impl fmt::Debug for AssetKeepAlive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AssetKeepAlive").field(&self.0.id).finish()
    }
}

impl hash::Hash for AssetKeepAlive {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

impl Eq for AssetKeepAlive {}

impl PartialEq for AssetKeepAlive {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

// === AssetManager Keys === //

pub trait AssetKeyOwned: 'static + fmt::Debug + Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

impl<T> AssetKeyOwned for T
where
    T: 'static + fmt::Debug + Send + Sync,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub trait AssetKey: Sized {
    type Owned: AssetKeyOwned;

    fn delegated(&self) -> impl AssetKey<Owned = Self::Owned> + '_;

    fn hash_key(&self, state: &mut impl hash::Hasher) {
        self.delegated().hash_key(state);
    }

    fn to_owned_key(&self) -> Self::Owned {
        self.delegated().to_owned_key()
    }

    fn matches_key(&self, owned: &Self::Owned) -> bool {
        self.delegated().matches_key(owned)
    }
}

impl<T: AssetKey> AssetKey for &'_ T {
    type Owned = T::Owned;

    fn delegated(&self) -> impl AssetKey<Owned = Self::Owned> + '_ {
        *self
    }

    fn hash_key(&self, state: &mut impl hash::Hasher) {
        (**self).hash_key(state);
    }

    fn to_owned_key(&self) -> Self::Owned {
        (**self).to_owned_key()
    }

    fn matches_key(&self, owned: &Self::Owned) -> bool {
        (*self).matches_key(owned)
    }
}

#[derive(Debug, Copy, Clone, Hash)]
pub struct RefKey<'a, T: ?Sized>(pub &'a T);

impl<T> AssetKey for RefKey<'_, T>
where
    T: ?Sized + hash::Hash + Eq + ToOwned,
    T::Owned: 'static + fmt::Debug + Send + Sync,
{
    type Owned = T::Owned;

    fn delegated(&self) -> impl AssetKey<Owned = Self::Owned> + '_ {
        self
    }

    fn hash_key(&self, state: &mut impl hash::Hasher) {
        self.hash(state);
    }

    fn to_owned_key(&self) -> Self::Owned {
        self.0.to_owned()
    }

    fn matches_key(&self, owned: &Self::Owned) -> bool {
        self.0 == owned.borrow()
    }
}

#[derive(Debug, Copy, Clone, Hash)]
pub struct OptionKey<'a, T: ?Sized>(pub Option<&'a T>);

impl<T> AssetKey for OptionKey<'_, T>
where
    T: ?Sized + hash::Hash + Eq + ToOwned,
    T::Owned: 'static + fmt::Debug + Send + Sync,
{
    type Owned = Option<T::Owned>;

    fn delegated(&self) -> impl AssetKey<Owned = Self::Owned> + '_ {
        self
    }

    fn hash_key(&self, state: &mut impl hash::Hasher) {
        self.0.hash(state);
    }

    fn to_owned_key(&self) -> Self::Owned {
        self.0.map(|v| v.to_owned())
    }

    fn matches_key(&self, owned: &Self::Owned) -> bool {
        self.0 == owned.as_ref().map(|v| v.borrow())
    }
}

#[derive(Debug, Copy, Clone, Hash)]
pub struct ListKey<'a, T>(pub &'a [&'a T]);

impl<T> AssetKey for ListKey<'_, T>
where
    T: hash::Hash + Eq + ToOwned,
    T::Owned: 'static + fmt::Debug + Send + Sync,
{
    type Owned = Vec<T::Owned>;

    fn delegated(&self) -> impl AssetKey<Owned = Self::Owned> + '_ {
        self
    }

    fn hash_key(&self, state: &mut impl hash::Hasher) {
        self.hash(state);
    }

    fn to_owned_key(&self) -> Self::Owned {
        self.0.iter().map(|v| (*v).to_owned()).collect()
    }

    fn matches_key(&self, owned: &Self::Owned) -> bool {
        if self.0.len() != owned.len() {
            return false;
        }

        self.0
            .iter()
            .zip(owned)
            .all(|(lhs, rhs)| *lhs == rhs.borrow())
    }
}

macro_rules! impl_asset_key {
    ($($name:ident:$field:tt),*) => {
        impl<$($name: AssetKey),*> AssetKey for ($($name,)*) {
            type Owned = ($($name::Owned,)*);

            fn delegated(&self) -> impl AssetKey<Owned = Self::Owned> + '_ {
                self
            }

            fn hash_key(&self, #[allow(unused)] state: &mut impl hash::Hasher) {
                $(self.$field.hash_key(state);)*
            }

            #[allow(clippy::unused_unit)]
            fn to_owned_key(&self) -> Self::Owned {
                ($(self.$field.to_owned_key(),)*)
            }

            fn matches_key(&self, #[allow(unused)] owned: &Self::Owned) -> bool {
                $(self.$field.matches_key(&owned.$field) && )* true
            }
        }
    };
}

impl_tuples!(impl_asset_key);

struct AssetKeyHashAdapter<'a, T>(&'a T);

impl<T: AssetKey> hash::Hash for AssetKeyHashAdapter<'_, T> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.0.hash_key(state);
    }
}

// === AssetRetainer === //

#[derive(Debug)]
pub struct AssetRetainer {
    manager: AssetManager,
    retained: FxHashMap<AssetKeepAlive, bool>,
}

impl AssetRetainer {
    pub fn new(manager: AssetManager) -> Self {
        Self {
            manager,
            retained: FxHashMap::default(),
        }
    }

    pub fn manager(&self) -> &AssetManager {
        &self.manager
    }

    pub fn reap(&mut self) {
        self.retained.retain(|_, is_kept| mem::take(is_kept));
    }
}

impl AssetLoader for AssetRetainer {
    fn manager(&self) -> &AssetManager {
        &self.manager
    }

    fn push_keep_alive(&mut self, keep_alive: &AssetKeepAlive) {
        match self.retained.get_mut(keep_alive) {
            Some(entry) => {
                *entry = true;
            }
            None => {
                self.retained.insert(keep_alive.clone(), true);
            }
        }
    }

    fn load<C, K, O>(
        &mut self,
        context: C,
        key: K,
        loader: fn(&mut AssetManagerTracked, C, K) -> O,
    ) -> Asset<O>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        let asset = self.manager().load_untracked(context, key, loader);
        self.push_keep_alive(Asset::keep_alive(&asset));
        asset
    }
}
