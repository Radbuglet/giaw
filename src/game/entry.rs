use crate::util::lang::entity::OwnedEntity;

use super::{
    actors::player::create_player,
    services::{actors::ActorManager, transform::Transform},
};

pub fn create_game_root() -> OwnedEntity {
    let root = OwnedEntity::new()
        .with_debug_label("game root")
        .with_cyclic(Transform::new_cyclic(None))
        .with(ActorManager::default());

    create_player(&mut root.get_mut::<ActorManager>(), Some(root.obj()));
    root
}
