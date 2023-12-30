use crate::{
    game::services::{actors::ActorManager, transform::Transform},
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
    fn new_cyclic() -> impl CyclicCtor<Self> {
        |me, _ob| Self {
            transform: me.obj(),
        }
    }

    pub fn update(&mut self) {}
}

pub fn create_player(actors: &mut ActorManager, parent: Option<Obj<Transform>>) -> Entity {
    actors
        .spawn()
        .with_debug_label("player")
        .with_cyclic(Transform::new_cyclic(parent))
        .with_cyclic(PlayerState::new_cyclic())
}
