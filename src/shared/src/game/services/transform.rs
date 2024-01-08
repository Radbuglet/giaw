use std::cell::{Cell, Ref, RefCell};

use aunty::{CyclicCtor, Entity, Obj, OpenCell};
use autoken::ImmutableBorrow;
use extend::ext;
use glam::{Affine2, Vec2};

use crate::util::math::aabb::Aabb;

// === Transform === //

#[derive(Debug)]
pub struct Transform {
    me: (Entity, Obj<Self>),
    parent: OpenCell<Option<Obj<Self>>>,
    children: RefCell<Vec<Obj<Self>>>,
    collider: OpenCell<Option<Obj<Collider>>>,
    index_in_parent: Cell<usize>,
    local_xform: Cell<Affine2>,
    global_xform: Cell<Affine2>,
}

impl Transform {
    // === Ancestry === //

    pub fn new(parent: Option<Obj<Self>>) -> impl CyclicCtor<Self> {
        |me, ob| {
            let mut index_in_parent = 0;
            if let Some(parent) = &parent {
                let parent = parent.get();
                let mut children = parent.children.borrow_mut();

                index_in_parent = children.len();
                children.push(ob.clone());
            }

            Self {
                me: (me, ob.clone()),
                parent: OpenCell::new(parent),
                children: RefCell::default(),
                collider: OpenCell::default(),
                index_in_parent: Cell::new(index_in_parent),
                local_xform: Cell::new(Affine2::IDENTITY),
                global_xform: Cell::new(Affine2::NAN),
            }
        }
    }

    pub fn parent(&self) -> Option<Obj<Self>> {
        self.parent.get()
    }

    pub fn set_parent(&self, parent: Option<Obj<Self>>) {
        if let Some(parent) = self.parent() {
            let parent = parent.get();
            let index_in_parent = self.index_in_parent.get();

            let mut children = parent.children.borrow_mut();
            children.swap_remove(index_in_parent);

            if let Some(moved) = children.get(index_in_parent) {
                moved.get().index_in_parent.set(index_in_parent);
            }
        }

        self.parent.set(parent);

        if let Some(parent) = self.parent() {
            let parent = parent.get();

            let mut children = parent.children.borrow_mut();
            self.index_in_parent.set(children.len());
            children.push(self.me.1.clone());
        }
    }

    pub fn children(&self) -> Ref<'_, [Obj<Self>]> {
        Ref::map(self.children.borrow(), Vec::as_slice)
    }

    pub fn entity(&self) -> Entity {
        self.me.0
    }

    pub fn deep_obj<T: 'static>(&self) -> Obj<T> {
        let loaner = ImmutableBorrow::<Self>::new();
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

    pub fn collider(&self) -> Option<Obj<Collider>> {
        self.collider.get()
    }

    pub(super) fn set_collider(&self, collider: Option<Obj<Collider>>) {
        self.collider.set(collider);
    }

    // === Transforms === //

    pub fn local_xform(&self) -> Affine2 {
        self.local_xform.get()
    }

    pub fn parent_xform(&self) -> Affine2 {
        self.parent()
            .map_or(Affine2::IDENTITY, |parent| parent.get().global_xform())
    }

    pub fn global_xform(&self) -> Affine2 {
        let mut global_xform = self.global_xform.get();
        if global_xform.is_nan() {
            global_xform = self.parent_xform() * self.local_xform();
            self.global_xform.set(global_xform);
        }

        global_xform
    }

    pub fn set_local_xform(&self, affine: Affine2) {
        self.local_xform.set(affine);
        self.invalidate_global_xform();
    }

    pub fn set_global_xform(&self, affine: Affine2) {
        self.local_xform.set(self.parent_xform().inverse() * affine);
        self.global_xform.set(affine);

        for child in self.children().iter() {
            child.get().invalidate_global_xform();
        }
    }

    pub fn invalidate_global_xform(&self) {
        if !self.global_xform.get().is_nan() {
            self.global_xform.set(Affine2::NAN);

            if let Some(collider) = self.collider() {
                collider.get().invalidate_global_aabb();
            }

            for child in self.children().iter() {
                child.get().invalidate_global_xform();
            }
        }
    }

    // === Transform helpers === //

    pub fn update_local_xform(&self, f: impl FnOnce(Affine2) -> Affine2) {
        self.set_local_xform(f(self.local_xform()));
    }

    pub fn update_global_xform(&self, f: impl FnOnce(Affine2) -> Affine2) {
        self.set_global_xform(f(self.global_xform()));
    }

    pub fn local_pos(&self) -> Vec2 {
        self.local_xform().translation
    }

    pub fn global_pos(&self) -> Vec2 {
        self.global_xform().translation
    }

    pub fn set_local_pos(&self, pos: Vec2) {
        self.update_local_xform(|mut xf| {
            xf.translation = pos;
            xf
        })
    }

    pub fn set_global_pos(&self, pos: Vec2) {
        self.update_global_xform(|mut xf| {
            xf.translation = pos;
            xf
        })
    }

    pub fn translate_local(&self, dt: Vec2) {
        self.update_local_xform(|mut xf| {
            xf.translation += dt;
            xf
        })
    }

    pub fn translate_global(&self, dt: Vec2) {
        self.update_global_xform(|mut xf| {
            xf.translation += dt;
            xf
        })
    }

    pub fn is_descendant_of(&self, other: &Obj<Self>) -> bool {
        let mut iter = self.me.1.clone();

        loop {
            if &iter == other {
                return true;
            }

            let Some(parent) = iter.get().parent() else {
                return false;
            };

            iter = parent;
        }
    }

    pub fn is_ancestor_of(&self, other: &Obj<Self>) -> bool {
        other.get().is_descendant_of(&self.me.1)
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
            transform.get().set_collider(Some(ob.clone()));

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
