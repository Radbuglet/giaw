use glam::Vec2;

use crate::{
    game::services::{
        kinematic::KinematicManager,
        transform::{Collider, EntityExt, Transform},
    },
    util::lang::{entity::CyclicCtor, obj::Obj},
};

#[derive(Debug)]
pub struct PlayerState {
    transform: Obj<Transform>,
    collider: Obj<Collider>,
    kine: Obj<KinematicManager>,
}

impl PlayerState {
    pub fn new() -> impl CyclicCtor<Self> {
        |me, _ob| Self {
            transform: me.obj(),
            collider: me.obj(),
            kine: me.deep_obj(),
        }
    }

    pub fn update(&mut self) {
        let xform = self.transform.get();
        let aabb = self.collider.get().global_aabb();

        xform.translate_local_pos(self.kine.get().move_by(
            aabb,
            Vec2::Y * 0.1,
            Some(&self.transform),
        ));
    }
}
