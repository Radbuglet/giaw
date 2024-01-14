use aunty::{CyclicCtor, Entity, Obj, StrongEntity};
use rustc_hash::FxHashMap;

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

#[derive(Debug)]
pub struct InventoryData {
    stacks: Box<[Option<Obj<ItemStackBase>>]>,
}

impl InventoryData {
    pub fn new(count: usize) -> Self {
        Self {
            stacks: Box::from_iter((0..count).map(|_| None)),
        }
    }

	pub fn insert_stack(&mut self, stack: Obj<ItemStackBase>) {
		for slot in &mut *self.stacks {
			if slot.is_none() {
				*slot = Some(stack);
				break;
			}
		}
	}

    pub fn stacks(&self) -> &[Option<Obj<ItemStackBase>>] {
        &self.stacks
    }
}

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
