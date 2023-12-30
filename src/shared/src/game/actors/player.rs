use glam::Vec2;

use crate::{
    game::services::{
        actors::{ActorManager, DespawnHandler},
        collider::Collider,
        transform::Transform,
    },
    util::lang::{
        entity::{CyclicCtor, Entity},
        obj::Obj,
    },
};

#[derive(Debug)]
pub struct PlayerState {
    transform: Obj<Transform>,
}

impl PlayerState {
    fn new() -> impl CyclicCtor<Self> {
        |me, _ob| Self {
            transform: me.obj(),
        }
    }

    pub fn update(&mut self) {
        let xform = self.transform.get();
        xform.set_local_pos(xform.local_pos() + Vec2::Y * 0.1);
    }
}

pub fn create_player(actors: &mut ActorManager, parent: Option<Obj<Transform>>) -> Entity {
    actors
        .spawn()
        .with_debug_label("player")
        .with_cyclic(Transform::new(parent))
        .with_cyclic(Collider::new_sized(Vec2::ZERO, Vec2::splat(2.)))
        .with_cyclic(PlayerState::new())
        .with_cyclic(|me, _| {
            DespawnHandler::new(move || {
                me.get::<Collider>().despawn();
            })
        })
}
