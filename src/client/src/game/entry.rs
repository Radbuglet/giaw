use giaw_shared::{
    game::services::{
        actors::{ActorManager, DespawnHandler, UpdateHandler},
        kinematic::{KinematicManager, TileColliderDescriptor},
        tile::{TileMap, TileLayerConfig},
        transform::{ColliderManager, Transform},
    },
    util::{
        lang::{entity::StrongEntity, obj::Obj},
        math::aabb::{Aabb, AabbI},
    },
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
            let layer = map.push_layer("under_player", TileLayerConfig::from_size(0.5));
            let placeholder;

            {
                let mut materials = map.materials.get_mut();
                materials.register("air", StrongEntity::new().with("air descriptor"));
                placeholder = materials.register(
                    "placeholder",
                    StrongEntity::new()
                        .with("placeholder descriptor")
                        .with(TileVisualDescriptor { color: GREEN })
                        .with(TileColliderDescriptor::new([Aabb::ZERO_TO_ONE])),
                );
            }

            for pos in AabbI::new_sized(IVec2::new(-10, 5), IVec2::new(20, 20))
                .inclusive()
                .iter()
            {
                map.set(layer, pos, placeholder);
            }
            map
        })
        .with_cyclic(KinematicManager::new())
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
