use giaw_shared::{
    game::{
        actors::player::PlayerState,
        services::{
            actors::{ActorManager, DespawnHandler, UpdateHandler},
            tile::TileMap,
            transform::{Collider, EntityExt, Transform},
        },
    },
    util::{
        lang::{entity::Entity, obj::Obj},
        math::aabb::Aabb,
    },
};
use macroquad::{
    color::{BLUE, RED},
    input::{is_key_down, is_mouse_button_down, mouse_position},
    math::Vec2,
    miniquad::{KeyCode, MouseButton},
    shapes::{draw_circle, draw_rectangle},
    time::get_frame_time,
};

use crate::{
    engine::scene::RenderHandler,
    game::services::camera::{CameraManager, VirtualCamera, VirtualCameraConstraints},
};

pub fn create_player(actors: &mut ActorManager, parent: Option<Obj<Transform>>) -> Entity {
    actors
        .spawn()
        .with_debug_label("player")
        .with_cyclic(Transform::new(parent))
        .with_cyclic(Collider::new_centered(Vec2::ZERO, Vec2::splat(0.6)))
        .with_cyclic(PlayerState::new())
        .with_cyclic(VirtualCamera::new_attached(
            Aabb::ZERO,
            VirtualCameraConstraints::default().keep_visible_area(Vec2::splat(10.)),
        ))
        // Handlers
        .with_cyclic(|me, _| {
            let player = me.obj::<PlayerState>();
            let camera_mgr = me.deep_obj::<CameraManager>();

            UpdateHandler::new(move || {
                let dt = get_frame_time();
                let mut player = player.get_mut();

                // Handle building
                {
                    let mouse_pos = camera_mgr.get_mut().project(mouse_position().into());
                    let mut tile_map = me.deep_obj::<TileMap>().get_mut();
                    let layer = tile_map.layer("under_player");
                    let tile_pos = tile_map.layers[layer.0].actor_to_tile(mouse_pos);

                    if is_mouse_button_down(MouseButton::Right) {
                        let material = tile_map.materials.get().get_by_name("placeholder");
                        tile_map.set(layer, tile_pos, material);
                    }

                    if is_mouse_button_down(MouseButton::Left) {
                        let material = tile_map.materials.get().get_by_name("air");
                        tile_map.set(layer, tile_pos, material);
                    }
                }

                // Handle motion
                {
                    let mut heading = 0.;
                    let magnitude = 5.;

                    if is_key_down(KeyCode::A) {
                        heading = -magnitude;
                    }

                    if is_key_down(KeyCode::D) {
                        heading = magnitude;
                    }

                    player.velocity.x = (player.velocity.x + heading) / 2.;

                    if is_key_down(KeyCode::Space) && player.is_on_ground() {
                        player.velocity.y = -10.;
                    }

                    player.update(dt);
                }
            })
        })
        .with_cyclic(|me, _| {
            let xform = me.obj::<Transform>();
            let camera_mgr = me.deep_obj::<CameraManager>();

            RenderHandler::new(move || {
                let xform = xform.get();
                let pos = xform.global_pos();

                {
                    let mouse_pos = camera_mgr.get_mut().project(mouse_position().into());
                    let tile_map = me.deep_obj::<TileMap>().get();
                    let layer = &tile_map.layers[tile_map.layer("under_player").0];
                    let aabb = layer.tile_to_actor_rect(layer.actor_to_tile(mouse_pos));

                    draw_rectangle(aabb.x(), aabb.y(), aabb.w(), aabb.h(), BLUE);
                }

                draw_circle(pos.x, pos.y, 0.3, RED);
            })
        })
        .with_cyclic(|me, _| {
            DespawnHandler::new(move || {
                me.get::<Collider>().despawn();
            })
        })
}
