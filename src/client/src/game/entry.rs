use giaw_shared::{
    game::{
        actors::player::create_player,
        services::{
            actors::{ActorManager, DespawnHandler, UpdateHandler},
            collider::ColliderManager,
            transform::Transform,
        },
    },
    util::lang::{entity::OwnedEntity, obj::Obj},
};

pub fn create_game(parent: Option<Obj<Transform>>) -> OwnedEntity {
    let scene = OwnedEntity::new()
        .with_debug_label("game scene root")
        .with_cyclic(Transform::new(parent))
        .with(ActorManager::default())
        .with(ColliderManager::default())
        .with_cyclic(|me, _| {
            UpdateHandler::new(move || {
                me.get_mut::<ActorManager>().process_despawns();
            })
        })
        .with_cyclic(|me, _| {
            DespawnHandler::new(move || {
                me.get_mut::<ActorManager>().despawn_all();
            })
        });

    create_player(&mut scene.get_mut(), Some(scene.obj()));
    scene
}
