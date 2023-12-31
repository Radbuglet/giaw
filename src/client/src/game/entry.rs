use giaw_shared::{
    game::services::{
        actors::{ActorManager, DespawnHandler, UpdateHandler},
        collider::ColliderManager,
        transform::Transform,
    },
    util::lang::{entity::StrongEntity, obj::Obj},
};
use macroquad::{
    camera::{pop_camera_state, push_camera_state, set_camera},
    math::Vec2,
    window::{screen_height, screen_width},
};

use crate::engine::scene::RenderHandler;

use super::{actors::player::create_player, services::camera::CameraManager};

pub fn create_game(parent: Option<Obj<Transform>>) -> StrongEntity {
    let scene = StrongEntity::new()
        .with_debug_label("game scene root")
        .with_cyclic(Transform::new(parent))
        .with(ActorManager::default())
        .with(ColliderManager::default())
        .with(CameraManager::default())
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
                push_camera_state();
                if let Some(camera) = me
                    .get_mut::<CameraManager>()
                    .camera_snapshot(Vec2::new(screen_width(), screen_height()))
                {
                    set_camera(&camera);
                } else {
                    eprintln!("No camera >:(");
                }

                cbit::cbit!(for actor in me.get::<ActorManager>().iter_actors() {
                    actor.get::<RenderHandler>().call();
                });

                pop_camera_state();
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
