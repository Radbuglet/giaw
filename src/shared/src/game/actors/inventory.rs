use aunty::{CyclicCtor, Entity, Obj, StrongEntity};
use rustc_hash::FxHashMap;

use crate::util::game::{actors::ActorManager, transform::Transform};

// === ItemRegistry === //

#[derive(Debug, Default)]
pub struct ItemRegistry {
    by_id: FxHashMap<String, StrongEntity>,
}

impl ItemRegistry {
    pub fn register(&mut self, id: impl Into<String>, descriptor: StrongEntity) -> Entity {
        let (descriptor_guard, descriptor) = descriptor.split_guard();
        self.by_id.insert(id.into(), descriptor_guard);
        descriptor
    }

    pub fn get(&self, id: &str) -> Entity {
        self.by_id[id].entity()
    }
}

// === InventoryData === //

#[derive(Debug)]
pub struct InventoryData {
    transform: Obj<Transform>,
    stacks: Box<[Option<Obj<ItemStackBase>>]>,
}

impl InventoryData {
    pub fn new(count: usize) -> impl CyclicCtor<Self> {
        move |me, _| Self {
            transform: me.obj(),
            stacks: Box::from_iter((0..count).map(|_| None)),
        }
    }

    pub fn insert_stack_raw(&mut self, stack: Obj<ItemStackBase>) {
        for slot in &mut *self.stacks {
            if slot.is_none() {
                *slot = Some(stack);
                break;
            }
        }
    }

    pub fn insert_stack(&mut self, actors: &ActorManager, material: Entity, count: u32) {
        self.insert_stack_raw(create_basic_stack(
            actors,
            Some(self.transform.clone()),
            material,
            count,
        ));
    }

    pub fn stacks(&self) -> &[Option<Obj<ItemStackBase>>] {
        &self.stacks
    }
}

// === ItemStackBase === //

#[derive(Debug)]
pub struct ItemStackBase {
    pub me: Entity,
    pub material: Entity,
    pub count: u32,
}

impl ItemStackBase {
    pub fn new(material: Entity, count: u32) -> impl CyclicCtor<Self> {
        move |me, _| Self {
            me,
            material,
            count,
        }
    }
}

// === ItemStack Prefab === //

pub fn create_basic_stack(
    actors: &ActorManager,
    parent: Option<Obj<Transform>>,
    material: Entity,
    count: u32,
) -> Obj<ItemStackBase> {
    actors
        .spawn()
        .with_debug_label("item stack")
        .with_cyclic(Transform::new(parent))
        .with_cyclic(ItemStackBase::new(material, count))
        .obj()
}
