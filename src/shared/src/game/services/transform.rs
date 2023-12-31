use std::cell::Cell;

use autoken::ImmutableBorrow;
use extend::ext;
use glam::{Affine2, Vec2};

use crate::util::{
    lang::{
        entity::{CyclicCtor, Entity},
        obj::Obj,
    },
    math::aabb::Aabb,
};

#[derive(Debug)]
pub struct Transform {
    me: Entity,
    parent: Option<Obj<Transform>>,
    children: Vec<Obj<Transform>>,
    index_in_parent: usize,
    collider: Option<Obj<Collider>>,
    local_xform: Cell<Affine2>,
    global_xform: Cell<Affine2>,
}

impl Transform {
    pub fn new(parent: Option<Obj<Transform>>) -> impl CyclicCtor<Self> {
        |me, ob| {
            let mut index_in_parent = 0;

            if let Some(parent) = &parent {
                let mut parent = parent.get_mut();
                index_in_parent = parent.children.len();
                parent.children.push(ob.clone());
            }

            Self {
                me,
                parent,
                children: Vec::new(),
                index_in_parent,
                collider: None,
                local_xform: Cell::new(Affine2::IDENTITY),
                global_xform: Cell::new(Affine2::NAN),
            }
        }
    }

    pub fn local_xform(&self) -> Affine2 {
        self.local_xform.get()
    }

    pub fn set_local_xform(&self, xform: Affine2) {
        self.local_xform.set(xform);
        self.invalidate_global_xform();
    }

    pub fn global_xform(&self) -> Affine2 {
        let mut xform = self.global_xform.get();
        if xform.is_nan() {
            let parent_xform = self
                .parent
                .as_ref()
                .map_or(Affine2::IDENTITY, |parent| parent.get().global_xform());

            xform = parent_xform * self.local_xform.get();
            self.global_xform.set(xform);
        }

        xform
    }

    pub fn invalidate_global_xform(&self) {
        if !self.global_xform.get().is_nan() {
            self.global_xform.set(Affine2::NAN);

            if let Some(collider) = &self.collider {
                collider.get().invalidate_global_aabb();
            }

            for child in &self.children {
                child.get().invalidate_global_xform();
            }
        }
    }

    pub fn local_pos(&self) -> Vec2 {
        self.local_xform().translation
    }

    pub fn set_local_pos(&self, pos: Vec2) {
        let mut xform = self.local_xform();
        xform.translation = pos;
        self.set_local_xform(xform);
    }

    pub fn translate_local_pos(&self, by: Vec2) {
        self.set_local_pos(self.local_pos() + by);
    }

    pub fn global_pos(&self) -> Vec2 {
        self.global_xform().translation
    }

    pub fn parent(&self) -> Option<&Obj<Transform>> {
        self.parent.as_ref()
    }

    pub fn children(&self) -> &[Obj<Transform>] {
        &self.children
    }

    pub fn entity(&self) -> Entity {
        self.me
    }

    pub fn collider(&self) -> Option<&Obj<Collider>> {
        self.collider.as_ref()
    }

    pub(super) fn set_collider(&mut self, collider: Option<Obj<Collider>>) {
        self.collider = collider;
    }

    pub fn deep_obj<T: 'static>(&self) -> Obj<T> {
        let loaner = ImmutableBorrow::<Transform>::new();
        let mut guard;
        let mut search = self;

        loop {
            if let Some(found) = search.entity().try_obj::<T>() {
                break found;
            }

            let Some(parent) = search.parent() else {
                panic!("failed to find component in ancestry");
            };

            guard = parent.get_on_loan(&loaner);
            search = &*guard;
        }
    }
}

#[ext]
pub impl Obj<Transform> {
    fn set_parent(&self, parent: Option<Obj<Transform>>) {
        let mut me = self.get_mut();

        if let Some(parent) = &me.parent {
            let mut parent = autoken::assume_no_alias(|| parent.get_mut());
            parent.children.swap_remove(me.index_in_parent);

            if let Some(moved) = parent.children.get(me.index_in_parent) {
                autoken::assume_no_alias(|| moved.get_mut()).index_in_parent = me.index_in_parent;
            }
        }

        me.parent = parent;

        if let Some(parent) = &me.parent {
            let mut parent = autoken::assume_no_alias(|| parent.get_mut());
            me.index_in_parent = parent.children.len();
            parent.children.push(self.clone());
        }
    }

    fn is_descendant_of(&self, other: &Obj<Transform>) -> bool {
        let mut iter = self.clone();

        loop {
            if &iter == other {
                return true;
            }

            let Some(parent) = iter.get().parent.clone() else {
                return false;
            };

            iter = parent;
        }
    }

    fn is_ancestor_of(&self, other: &Obj<Transform>) -> bool {
        other.is_descendant_of(self)
    }
}

#[ext]
pub impl Entity {
    fn deep_obj<T: 'static>(self) -> Obj<T> {
        self.get::<Transform>().deep_obj()
    }
}

// === Collider === //

fn compute_global_aabb(global_xform: Affine2, local_aabb: Aabb) -> Aabb {
    Aabb {
        min: global_xform.transform_point2(local_aabb.min),
        max: global_xform.transform_point2(local_aabb.max),
    }
}

#[derive(Debug, Default)]
pub struct ColliderManager {
    colliders: Vec<Obj<Collider>>,
}

impl ColliderManager {
    pub fn iter_in(&self, aabb: Aabb) -> impl Iterator<Item = (Entity, &Obj<Collider>, Aabb)> + '_ {
        self.colliders.iter().filter_map(move |collider| {
            let collider_info = collider.get();
            let their_aabb = collider_info.global_aabb();
            aabb.intersects(their_aabb)
                .then(|| (collider_info.entity(), collider, aabb))
        })
    }
}

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

    pub fn transform(&self) -> &Obj<Transform> {
        &self.transform
    }
}

impl Drop for Collider {
    fn drop(&mut self) {
        assert_eq!(self.index_in_manager.get(), usize::MAX);
    }
}
