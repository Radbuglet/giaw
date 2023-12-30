use std::cell::Cell;

use extend::ext;
use glam::{Affine2, Vec2};

use crate::util::lang::{
    entity::{CyclicCtor, Entity},
    obj::Obj,
};

use super::collider::Collider;

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
        let mut guard;
        let mut search = self;

        loop {
            if let Some(found) = search.entity().try_obj::<T>() {
                break found;
            }

            let Some(parent) = search.parent() else {
                panic!("failed to find component in ancestry");
            };

            guard = parent.get();
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
}

#[ext]
pub impl Entity {
    fn deep_obj<T: 'static>(self) -> Obj<T> {
        self.get::<Transform>().deep_obj()
    }
}
