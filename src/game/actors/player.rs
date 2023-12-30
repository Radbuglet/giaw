use crate::{
    game::services::{actors::ActorManager, transform::Transform},
    util::lang::{
        entity::{cyclic_ctor, CyclicCtor, Entity},
        obj::Obj,
    },
};

#[derive(Debug)]
pub struct PlayerState {
    transform: Obj<Transform>,
}

impl PlayerState {
    fn new_cyclic() -> impl CyclicCtor<Self> {
        cyclic_ctor(|me, _ob| Self {
            transform: me.obj(),
        })
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
