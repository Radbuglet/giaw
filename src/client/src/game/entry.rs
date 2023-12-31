use giaw_shared::{
    game::services::{
        actors::{ActorManager, DespawnHandler, UpdateHandler},
        collider::ColliderManager,
        tile::TileMap,
        transform::Transform,
    },
    util::lang::{entity::StrongEntity, obj::Obj},
};
use macroquad::{color::GREEN, math::IVec2};

use crate::engine::scene::RenderHandler;

use super::{
    actors::player::create_player,
    services::{
        camera::CameraManager,
        render::{TileVisualDescriptor, WorldRenderer},
    },
};

pub fn create_game(parent: Option<Obj<Transform>>) -> StrongEntity {
    let scene = StrongEntity::new()
        .with_debug_label("game scene root")
        .with_cyclic(Transform::new(parent))
        .with(ActorManager::default())
        .with(ColliderManager::default())
        .with(CameraManager::default())
        .with_cyclic(|_, _| {
            let mut map = TileMap::default();
            let layer = map.push_layer("under_player", 10.);
            let placeholder;

            {
                let mut materials = map.materials.get_mut();
                materials.register("air", StrongEntity::new().with("air descriptor"));
                placeholder = materials.register(
                    "placeholder",
                    StrongEntity::new()
                        .with("placeholder descriptor")
                        .with(TileVisualDescriptor { color: GREEN }),
                );
            }

            map.set(layer, IVec2::ZERO, placeholder);
            map
        })
        .with_cyclic(WorldRenderer::new())
        .with_cyclic(|me, _| {
            UpdateHandler::new(move || {
                let actor_mgr = me.get::<ActorManager>();

                cbit::cbit!(for actor in actor_mgr.iter_actors() {
                    actor.get::<UpdateHandler>().call();
                });

                actor_mgr.process_despawns();
            })
        })
        .with_cyclic(|me, _| {
            RenderHandler::new(move || {
                me.get::<WorldRenderer>().render();
            })
        })
        .with_cyclic(|me, _| {
            DespawnHandler::new(move || {
                me.get::<ActorManager>().despawn_all();
            })
        });

    create_player(&mut scene.get_mut(), Some(scene.obj()));
    scene
}
