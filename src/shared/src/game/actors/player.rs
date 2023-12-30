use glam::Vec2;

use crate::{
    game::services::transform::Transform,
    util::lang::{entity::CyclicCtor, obj::Obj},
};

#[derive(Debug)]
pub struct PlayerState {
    transform: Obj<Transform>,
}

impl PlayerState {
    pub fn new() -> impl CyclicCtor<Self> {
        |me, _ob| Self {
            transform: me.obj(),
        }
    }

    pub fn update(&mut self) {
        let xform = self.transform.get();
        xform.set_local_pos(xform.local_pos() + Vec2::Y * 0.1);
    }
}
