use std::cell::Cell;

use glam::{Affine2, Vec2};

use crate::util::{
    lang::{
        entity::{CyclicCtor, Entity},
        obj::Obj,
    },
    math::aabb::Aabb,
};

use super::transform::Transform;

// === Utils === //

fn compute_global_aabb(global_xform: Affine2, local_aabb: Aabb) -> Aabb {
    Aabb {
        min: global_xform.transform_point2(local_aabb.min),
        max: global_xform.transform_point2(local_aabb.max),
    }
}

// === ColliderManager === //

#[derive(Debug, Default)]
pub struct ColliderManager {
    colliders: Vec<Obj<Collider>>,
}

impl ColliderManager {
    pub fn iter_in(&self, aabb: Aabb) -> impl Iterator<Item = &Obj<Collider>> + '_ {
        self.colliders.iter().filter(move |collider| {
            let their_aabb = collider.get().global_aabb();
            aabb.intersects(their_aabb)
        })
    }
}

// === Collider === //

#[derive(Debug)]
pub struct Collider {
    // Cached references
    me: Entity,
    transform: Obj<Transform>,
    manager: Obj<ColliderManager>,

    // State
    index_in_manager: Cell<usize>,
    local_aabb: Cell<Aabb>,
    global_aabb: Cell<Aabb>,
}

impl Collider {
    pub fn new(aabb: Aabb) -> impl CyclicCtor<Self> {
        move |me, ob| {
            // Link dependencies
            let transform = me.obj::<Transform>();
            let manager = transform.get().deep_obj::<ColliderManager>();
            transform.get_mut().set_collider(Some(ob.clone()));

            // Add to manager
            let mut manager_mut = manager.get_mut();
            let index_in_manager = manager_mut.colliders.len();
            manager_mut.colliders.push(ob.clone());
            drop(manager_mut);

            Self {
                me,
                transform,
                manager,
                index_in_manager: Cell::new(index_in_manager),
                local_aabb: Cell::new(aabb),
                global_aabb: Cell::new(Aabb::NAN),
            }
        }
    }

    pub fn new_centered(offset: Vec2, size: Vec2) -> impl CyclicCtor<Self> {
        Self::new(Aabb::new_centered(offset, size))
    }

    pub fn despawn(&self) {
        let mut manager = self.manager.get_mut();
        let index_in_manager = self.index_in_manager.get();
        manager.colliders.swap_remove(index_in_manager);

        if let Some(moved) = manager.colliders.get(index_in_manager) {
            moved.get().index_in_manager.set(index_in_manager);
        }

        self.index_in_manager.set(usize::MAX);
    }

    pub fn entity(&self) -> Entity {
        self.me
    }

    pub fn local_aabb(&self) -> Aabb {
        self.local_aabb.get()
    }

    pub fn set_local_aabb(&self, aabb: Aabb) {
        self.local_aabb.set(aabb);
        self.invalidate_global_aabb();
    }

    pub fn global_aabb(&self) -> Aabb {
        let mut aabb = self.global_aabb.get();
        if aabb.is_nan() {
            aabb = compute_global_aabb(self.transform.get().global_xform(), self.local_aabb());
            self.global_aabb.set(aabb);
        }

        aabb
    }

    pub fn invalidate_global_aabb(&self) {
        self.global_aabb.set(Aabb::NAN);
    }
}

impl Drop for Collider {
    fn drop(&mut self) {
        assert_eq!(self.index_in_manager.get(), usize::MAX);
    }
}
