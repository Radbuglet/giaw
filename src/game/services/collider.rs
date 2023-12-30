use crate::util::lang::{entity::CyclicCtor, obj::Obj};

use super::transform::EntityExt;

#[derive(Debug)]
pub struct ColliderManager {}

#[derive(Debug)]
pub struct Collider {
    manager: Obj<ColliderManager>,
}

impl Collider {
    pub fn new_cyclic() -> impl CyclicCtor<Self> {
        |me, _| Self {
            manager: me.deep_obj::<ColliderManager>(),
        }
    }
}
