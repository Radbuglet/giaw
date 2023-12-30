use std::{
    any::{type_name, Any, TypeId},
    borrow::{Borrow, Cow},
    cell::{Cell, Ref, RefCell},
    fmt, hash, iter, mem,
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

use autoken::{ImmutableBorrow, MutableBorrow, Nothing};
use debug::{AsDebugLabel, DebugLabel};

use super::obj::{CompMut, CompRef, Obj, StrongObj};

// === Helpers === //

fn xorshift64(state: NonZeroU64) -> NonZeroU64 {
    // Adapted from: https://en.wikipedia.org/w/index.php?title=Xorshift&oldid=1123949358
    let state = state.get();
    let state = state ^ (state << 13);
    let state = state ^ (state >> 7);
    let state = state ^ (state << 17);
    NonZeroU64::new(state).unwrap()
}

type NopHashBuilder = hash::BuildHasherDefault<NoOpHasher>;
type NopHashMap<K, V> = hashbrown::HashMap<K, V, NopHashBuilder>;

type FxHashBuilder = hash::BuildHasherDefault<rustc_hash::FxHasher>;
type FxHashMap<K, V> = hashbrown::HashMap<K, V, FxHashBuilder>;
type FxHashSet<T> = hashbrown::HashSet<T, FxHashBuilder>;

fn hash_iter<H, E, I>(state: &mut H, iter: I)
where
    H: hash::Hasher,
    E: hash::Hash,
    I: IntoIterator<Item = E>,
{
    for item in iter {
        item.hash(state);
    }
}

fn merge_iters<I, A, B>(a: A, b: B) -> impl Iterator<Item = I>
where
    I: Ord,
    A: IntoIterator<Item = I>,
    B: IntoIterator<Item = I>,
{
    let mut a_iter = a.into_iter().peekable();
    let mut b_iter = b.into_iter().peekable();

    iter::from_fn(move || {
        // Unfortunately, `Option`'s default Ord impl isn't suitable for this.
        match (a_iter.peek(), b_iter.peek()) {
            (Some(a), Some(b)) => {
                if a < b {
                    a_iter.next()
                } else {
                    b_iter.next()
                }
            }
            (Some(_), None) => a_iter.next(),
            (None, Some(_)) => b_iter.next(),
            (None, None) => None,
        }
    })
}

fn leak<T>(value: T) -> &'static T {
    Box::leak(Box::new(value))
}

#[derive(Debug, Default)]
struct NoOpHasher(u64);

impl hash::Hasher for NoOpHasher {
    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }

    fn write(&mut self, _bytes: &[u8]) {
        unimplemented!("This is only supported for `u64`s.")
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

// === ComponentList === //

#[derive(Copy, Clone)]
struct ComponentType {
    id: TypeId,
    name: &'static str,
    dtor: fn(Entity),
}

impl ComponentType {
    fn of<T: 'static>() -> Self {
        fn dtor<T: 'static>(entity: Entity) {
            drop(storage::<T>().remove_untracked(entity)); // (ignores missing components)
        }

        Self {
            id: TypeId::of::<T>(),
            name: type_name::<T>(),
            dtor: dtor::<T>,
        }
    }
}

impl Ord for ComponentType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl PartialOrd for ComponentType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl hash::Hash for ComponentType {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Eq for ComponentType {}

impl PartialEq for ComponentType {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

struct ComponentList {
    comps: Box<[ComponentType]>,
    extensions: RefCell<FxHashMap<TypeId, &'static Self>>,
    de_extensions: RefCell<FxHashMap<TypeId, &'static Self>>,
}

impl hash::Hash for ComponentList {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        hash_iter(state, self.comps.iter());
    }
}

impl Eq for ComponentList {}

impl PartialEq for ComponentList {
    fn eq(&self, other: &Self) -> bool {
        self.comps == other.comps
    }
}

impl ComponentList {
    pub fn empty() -> &'static Self {
        thread_local! {
            static EMPTY: &'static ComponentList = leak(ComponentList {
                comps: Box::new([]),
                extensions: Default::default(),
                de_extensions: Default::default(),
            });
        }

        EMPTY.with(|v| *v)
    }

    pub fn run_dtors(&self, target: Entity) {
        for comp in &*self.comps {
            (comp.dtor)(target);
        }
    }

    pub fn extend(&'static self, with: ComponentType) -> &'static Self {
        if self.comps.contains(&with) {
            return self;
        }

        self.extensions
            .borrow_mut()
            .entry(with.id)
            .or_insert_with(|| Self::find_extension_in_db(&self.comps, with))
    }

    pub fn de_extend(&'static self, without: ComponentType) -> &'static Self {
        if !self.comps.contains(&without) {
            return self;
        }

        self.de_extensions
            .borrow_mut()
            .entry(without.id)
            .or_insert_with(|| Self::find_de_extension_in_db(&self.comps, without))
    }

    // === Database === //

    thread_local! {
        static COMP_LISTS: RefCell<FxHashSet<&'static ComponentList>> = RefCell::new(FxHashSet::from_iter([
            ComponentList::empty(),
        ]));
    }

    fn find_extension_in_db(base_set: &[ComponentType], with: ComponentType) -> &'static Self {
        struct ComponentListSearch<'a>(&'a [ComponentType], ComponentType);

        impl hash::Hash for ComponentListSearch<'_> {
            fn hash<H: hash::Hasher>(&self, state: &mut H) {
                hash_iter(state, merge_iters(self.0, &[self.1]));
            }
        }

        impl hashbrown::Equivalent<&'static ComponentList> for ComponentListSearch<'_> {
            fn equivalent(&self, key: &&'static ComponentList) -> bool {
                // See if the key component list without the additional component
                // is equal to the base list.
                key.comps.iter().filter(|v| **v == self.1).eq(self.0.iter())
            }
        }

        ComponentList::COMP_LISTS.with(|set| {
            *set.borrow_mut()
                .get_or_insert_with(&ComponentListSearch(base_set, with), |_| {
                    leak(Self {
                        comps: merge_iters(base_set.iter().copied(), [with])
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                        extensions: Default::default(),
                        de_extensions: Default::default(),
                    })
                })
        })
    }

    fn find_de_extension_in_db(
        base_set: &[ComponentType],
        without: ComponentType,
    ) -> &'static Self {
        struct ComponentListSearch<'a>(&'a [ComponentType], ComponentType);

        impl hash::Hash for ComponentListSearch<'_> {
            fn hash<H: hash::Hasher>(&self, state: &mut H) {
                hash_iter(state, self.0.iter().filter(|v| **v != self.1));
            }
        }

        impl hashbrown::Equivalent<&'static ComponentList> for ComponentListSearch<'_> {
            fn equivalent(&self, key: &&'static ComponentList) -> bool {
                // See if the base component list without the removed component
                // is equal to the key list.
                self.0.iter().filter(|v| **v == self.1).eq(key.comps.iter())
            }
        }

        ComponentList::COMP_LISTS.with(|set| {
            *set.borrow_mut()
                .get_or_insert_with(&ComponentListSearch(base_set, without), |_| {
                    leak(Self {
                        comps: base_set
                            .iter()
                            .copied()
                            .filter(|v| *v != without)
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                        extensions: Default::default(),
                        de_extensions: Default::default(),
                    })
                })
        })
    }
}

// === Storage === //

thread_local! {
    static STORAGES: RefCell<FxHashMap<TypeId, &'static dyn Any>> = Default::default();
}

pub fn storage<T: 'static>() -> &'static Storage<T> {
    STORAGES.with(|db| {
        db.borrow_mut()
            .entry(TypeId::of::<T>())
            .or_insert_with(|| {
                leak(Storage::<T>(RefCell::new(StorageInner {
                    mappings: NopHashMap::default(),
                })))
            })
            .downcast_ref::<Storage<T>>()
            .unwrap()
    })
}

#[derive(Debug)]
pub struct Storage<T: 'static>(RefCell<StorageInner<T>>);

#[derive(Debug)]
struct StorageInner<T: 'static> {
    mappings: NopHashMap<Entity, StrongObj<T>>,
}

impl<T: 'static> Storage<T> {
    pub fn acquire() -> &'static Storage<T> {
        storage()
    }

    pub fn insert(&self, entity: Entity, value: StrongObj<T>) -> Option<StrongObj<T>> {
        ALIVE.with(|slots| {
            let mut slots = slots.borrow_mut();
            let slot = slots.get_mut(&entity).unwrap_or_else(|| {
                panic!(
                    "attempted to attach a component of type {} to the dead or cross-thread {:?}.",
                    type_name::<T>(),
                    entity
                )
            });

            *slot = slot.extend(ComponentType::of::<T>());
        });

        self.insert_untracked(entity, value)
    }

    fn insert_untracked(&self, entity: Entity, value: StrongObj<T>) -> Option<StrongObj<T>> {
        match self.0.borrow_mut().mappings.entry(entity) {
            hashbrown::hash_map::Entry::Occupied(mut entry) => Some(entry.insert(value)),
            hashbrown::hash_map::Entry::Vacant(entry) => {
                entry.insert(value);
                None
            }
        }
    }

    pub fn remove(&self, entity: Entity) -> Option<StrongObj<T>> {
        if let Some(removed) = self.remove_untracked(entity) {
            // Modify the component list or fail silently if the entity lacks the component.
            // This behavior allows users to `remove` components explicitly from entities that are
            // in the of being destroyed. This is the opposite behavior of `insert`, which requires
            // the entity to be valid before modifying it. This pairing ensures that, by the time
            // `Entity::destroy()` resolves, all of the entity's components will have been removed.
            ALIVE.with(|slots| {
                let mut slots = slots.borrow_mut();
                let Some(slot) = slots.get_mut(&entity) else {
                    return;
                };

                *slot = slot.de_extend(ComponentType::of::<T>());
            });

            Some(removed)
        } else {
            // Only if the component is missing will we issue the standard error.
            assert!(
                entity.is_alive(),
                "attempted to remove a component of type {} from the already fully-dead or cross-thread {:?}",
                type_name::<T>(),
                entity,
            );
            None
        }
    }

    fn remove_untracked(&self, entity: Entity) -> Option<StrongObj<T>> {
        self.0.borrow_mut().mappings.remove(&entity)
    }

    fn try_obj_inner(&self, entity: Entity) -> Option<Ref<'_, StrongObj<T>>> {
        Ref::filter_map(self.0.borrow(), |me| me.mappings.get(&entity)).ok()
    }

    fn obj_inner(&self, entity: Entity) -> Ref<'_, StrongObj<T>> {
        self.try_obj_inner(entity).unwrap_or_else(|| {
            if entity.is_alive() {
                panic!(
                    "{entity:?} does not have a component of type {}",
                    type_name::<T>()
                );
            } else {
                panic!("{entity:?} is dead");
            }
        })
    }

    pub fn try_obj(&self, entity: Entity) -> Option<Obj<T>> {
        self.try_obj_inner(entity).map(|v| v.downgrade())
    }

    pub fn obj(&self, entity: Entity) -> Obj<T> {
        self.obj_inner(entity).downgrade()
    }

    pub fn try_get<'l>(
        &self,
        entity: Entity,
        loaner: &'l ImmutableBorrow<T>,
    ) -> Option<CompRef<T, Nothing<'l>>> {
        self.try_obj_inner(entity)
            .map(|obj| obj.get_on_loan(loaner))
    }

    pub fn try_get_mut<'l>(
        &self,
        entity: Entity,
        loaner: &'l mut MutableBorrow<T>,
    ) -> Option<CompMut<T, Nothing<'l>>> {
        self.try_obj_inner(entity)
            .map(|obj| obj.get_mut_on_loan(loaner))
    }

    pub fn get(&self, entity: Entity) -> CompRef<T> {
        self.obj_inner(entity).get()
    }

    pub fn get_mut(&self, entity: Entity) -> CompMut<T> {
        self.obj_inner(entity).get_mut()
    }

    pub fn get_on_loan<'l>(
        &self,
        entity: Entity,
        loaner: &'l ImmutableBorrow<T>,
    ) -> CompRef<T, Nothing<'l>> {
        self.obj_inner(entity).get_on_loan(loaner)
    }

    pub fn get_mut_on_loan<'l>(
        &self,
        entity: Entity,
        loaner: &'l mut MutableBorrow<T>,
    ) -> CompMut<T, Nothing<'l>> {
        self.obj_inner(entity).get_mut_on_loan(loaner)
    }

    pub fn has(&self, entity: Entity) -> bool {
        self.try_obj(entity).is_some()
    }
}

// === Entity === //

thread_local! {
    static ALIVE: RefCell<NopHashMap<Entity, &'static ComponentList>> = Default::default();
}

static DEBUG_ENTITY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub struct Entity(NonZeroU64);

impl Entity {
    pub fn new_unmanaged() -> Self {
        // Increment the total entity counter
        DEBUG_ENTITY_COUNTER.fetch_add(1, Ordering::Relaxed);

        // Allocate a slot
        thread_local! {
            static ID_GEN: Cell<NonZeroU64> = const { Cell::new(match NonZeroU64::new(1) {
                Some(v) => v,
                None => unreachable!(),
            }) };
        }

        let me = Self(ID_GEN.with(|v| {
            // N.B. `xorshift`, like all other well-constructed LSFRs, produces a full cycle of non-zero
            // values before repeating itself. Thus, this is an effective way to generate random but
            // unique IDs without using additional storage.
            let state = xorshift64(v.get());
            v.set(state);
            state
        }));

        // Register our slot in the alive set
        ALIVE.with(|slots| slots.borrow_mut().insert(me, ComponentList::empty()));

        me
    }

    pub fn with<T: 'static>(self, comp: T) -> Self {
        self.insert(StrongObj::new(comp));
        self
    }

    pub fn with_cyclic<T: 'static>(self, f: impl FnOnce(Entity, Obj<T>) -> T) -> Self {
        self.insert(StrongObj::new_cyclic(|ob| f(self, ob)));
        self
    }

    pub fn with_raw<T: 'static>(self, comp: StrongObj<T>) -> Self {
        self.insert(comp);
        self
    }

    pub fn with_debug_label<L: AsDebugLabel>(self, label: L) -> Self {
        #[cfg(debug_assertions)]
        self.with(DebugLabel::from(label));
        #[cfg(not(debug_assertions))]
        let _ = label;
        self
    }

    pub fn insert<T: 'static>(self, comp: StrongObj<T>) -> Option<StrongObj<T>> {
        storage::<T>().insert(self, comp)
    }

    pub fn remove<T: 'static>(self) -> Option<StrongObj<T>> {
        storage::<T>().remove(self)
    }

    pub fn try_obj<T: 'static>(self) -> Option<Obj<T>> {
        storage::<T>().try_obj(self)
    }

    pub fn obj<T: 'static>(self) -> Obj<T> {
        storage::<T>().obj(self)
    }

    pub fn get<T: 'static>(self) -> CompRef<T> {
        storage::<T>().get(self)
    }

    pub fn get_mut<T: 'static>(self) -> CompMut<T> {
        storage::<T>().get_mut(self)
    }

    pub fn try_get<T: 'static>(
        self,
        loaner: &ImmutableBorrow<T>,
    ) -> Option<CompRef<T, Nothing<'_>>> {
        storage::<T>().try_get(self, loaner)
    }

    pub fn try_get_mut<T: 'static>(
        self,
        loaner: &mut MutableBorrow<T>,
    ) -> Option<CompMut<T, Nothing<'_>>> {
        storage::<T>().try_get_mut(self, loaner)
    }

    pub fn get_on_loan<T: 'static>(self, loaner: &ImmutableBorrow<T>) -> CompRef<T, Nothing<'_>> {
        storage::<T>().get_on_loan(self, loaner)
    }

    pub fn get_mut_on_loan<T: 'static>(
        self,
        loaner: &mut MutableBorrow<T>,
    ) -> CompMut<T, Nothing<'_>> {
        storage::<T>().get_mut_on_loan(self, loaner)
    }

    pub fn has<T: 'static>(self) -> bool {
        storage::<T>().has(self)
    }

    pub fn is_alive(self) -> bool {
        ALIVE.with(|slots| slots.borrow().contains_key(&self))
    }

    pub fn destroy(self) {
        ALIVE.with(|slots| {
            let comp_list = slots.borrow_mut().remove(&self).unwrap_or_else(|| {
                panic!(
                    "attempted to destroy the already-dead or cross-threaded {:?}.",
                    self
                )
            });

            comp_list.run_dtors(self);
        });
    }
}

impl fmt::Debug for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct StrLit<'a>(&'a str);

        impl fmt::Debug for StrLit<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.0)
            }
        }

        ALIVE.with(|alive| {
            #[derive(Debug)]
            struct Id(NonZeroU64);

            if let Some(comp_list) = alive.borrow().get(self).copied() {
                let mut builder = f.debug_tuple("Entity");

                let label_loaner = ImmutableBorrow::new();
                if let Some(label) = self.try_get::<DebugLabel>(&label_loaner) {
                    builder.field(&label);
                }

                builder.field(&Id(self.0));

                for v in comp_list.comps.iter() {
                    if v.id != TypeId::of::<DebugLabel>() {
                        builder.field(&StrLit(v.name));
                    }
                }

                builder.finish()
            } else {
                f.debug_tuple("Entity")
                    .field(&"<DEAD OR CROSS-THREAD>")
                    .field(&Id(self.0))
                    .finish()
            }
        })
    }
}

// === OwnedEntity === //

#[derive(Debug, Hash, Eq, PartialEq)]
pub struct OwnedEntity(Entity);

impl Default for OwnedEntity {
    fn default() -> Self {
        Self::new()
    }
}

impl OwnedEntity {
    // === Lifecycle === //

    pub fn new() -> Self {
        Self(Entity::new_unmanaged())
    }

    pub fn entity(&self) -> Entity {
        self.0
    }

    pub fn unmanage(self) -> Entity {
        let entity = self.0;
        mem::forget(self);

        entity
    }

    pub fn split_guard(self) -> (Self, Entity) {
        let entity = self.entity();
        (self, entity)
    }

    // === Forwards === //

    pub fn with<T: 'static>(self, comp: T) -> Self {
        self.0.with(comp);
        self
    }

    pub fn with_cyclic<T: 'static>(self, f: impl FnOnce(Entity, Obj<T>) -> T) -> Self {
        self.0.with_cyclic(f);
        self
    }

    pub fn with_raw<T: 'static>(self, comp: StrongObj<T>) -> Self {
        self.0.with_raw(comp);
        self
    }

    pub fn with_debug_label<L: AsDebugLabel>(self, label: L) -> Self {
        self.0.with_debug_label(label);
        self
    }

    pub fn insert<T: 'static>(&self, comp: StrongObj<T>) -> Option<StrongObj<T>> {
        self.0.insert(comp)
    }

    pub fn remove<T: 'static>(&self) -> Option<StrongObj<T>> {
        self.0.remove()
    }

    pub fn try_obj<T: 'static>(&self) -> Option<Obj<T>> {
        self.0.try_obj()
    }

    pub fn obj<T: 'static>(&self) -> Obj<T> {
        self.0.obj()
    }

    pub fn get<T: 'static>(&self) -> CompRef<T> {
        self.0.get()
    }

    pub fn get_mut<T: 'static>(&self) -> CompMut<T> {
        self.0.get_mut()
    }

    pub fn try_get<'l, T: 'static>(
        &self,
        loaner: &'l ImmutableBorrow<T>,
    ) -> Option<CompRef<T, Nothing<'l>>> {
        self.0.try_get(loaner)
    }

    pub fn try_get_mut<'l, T: 'static>(
        &self,
        loaner: &'l mut MutableBorrow<T>,
    ) -> Option<CompMut<T, Nothing<'l>>> {
        self.0.try_get_mut(loaner)
    }

    pub fn get_on_loan<'l, T: 'static>(
        &self,
        loaner: &'l ImmutableBorrow<T>,
    ) -> CompRef<T, Nothing<'l>> {
        self.0.get_on_loan(loaner)
    }

    pub fn get_mut_on_loan<'l, T: 'static>(
        &self,
        loaner: &'l mut MutableBorrow<T>,
    ) -> CompMut<T, Nothing<'l>> {
        self.0.get_mut_on_loan(loaner)
    }

    pub fn has<T: 'static>(&self) -> bool {
        self.0.has::<T>()
    }

    pub fn is_alive(&self) -> bool {
        self.0.is_alive()
    }

    pub fn destroy(self) {
        drop(self);
    }
}

impl Borrow<Entity> for OwnedEntity {
    fn borrow(&self) -> &Entity {
        &self.0
    }
}

impl Drop for OwnedEntity {
    fn drop(&mut self) {
        self.0.destroy();
    }
}

// === Debug utilities === //

pub mod debug {

    use super::*;

    pub fn alive_entity_count() -> usize {
        ALIVE.with(|slots| slots.borrow().len())
    }

    pub fn alive_entities() -> Vec<Entity> {
        ALIVE.with(|slots| slots.borrow().keys().copied().collect())
    }

    pub fn spawned_entity_count() -> u64 {
        DEBUG_ENTITY_COUNTER.load(Ordering::Relaxed)
    }

    #[derive(Debug, Clone)]
    pub struct DebugLabel(pub Cow<'static, str>);

    impl<L: AsDebugLabel> From<L> for DebugLabel {
        fn from(value: L) -> Self {
            Self(AsDebugLabel::reify(value))
        }
    }

    pub trait AsDebugLabel {
        fn reify(me: Self) -> Cow<'static, str>;
    }

    impl AsDebugLabel for &'static str {
        fn reify(me: Self) -> Cow<'static, str> {
            Cow::Borrowed(me)
        }
    }

    impl AsDebugLabel for String {
        fn reify(me: Self) -> Cow<'static, str> {
            Cow::Owned(me)
        }
    }

    impl AsDebugLabel for fmt::Arguments<'_> {
        fn reify(me: Self) -> Cow<'static, str> {
            if let Some(str) = me.as_str() {
                Cow::Borrowed(str)
            } else {
                Cow::Owned(me.to_string())
            }
        }
    }

    impl AsDebugLabel for Cow<'static, str> {
        fn reify(me: Self) -> Cow<'static, str> {
            me
        }
    }
}
