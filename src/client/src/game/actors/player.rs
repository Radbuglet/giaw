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
        math::{aabb::Aabb, scalar::lerp_f32},
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

#[derive(Debug, Default)]
pub struct PlayerClientState {
    last_build_pos: Vec2,
    last_build_state: bool,
}

pub fn create_player(actors: &mut ActorManager, parent: Option<Obj<Transform>>) -> Entity {
    actors
        .spawn()
        .with_debug_label("player")
        .with_cyclic(Transform::new(parent))
        .with_cyclic(Collider::new_centered(Vec2::ZERO, Vec2::splat(0.6)))
        .with_cyclic(PlayerState::new())
        .with(PlayerClientState::default())
        .with_cyclic(VirtualCamera::new_attached(
            Aabb::ZERO,
            VirtualCameraConstraints::default().keep_visible_area(Vec2::splat(10.)),
        ))
        // Handlers
        .with_cyclic(|me, _| {
            let player = me.obj::<PlayerState>();
            let player_client = me.obj::<PlayerClientState>();
            let camera_mgr = me.deep_obj::<CameraManager>();
            let tile_map = me.deep_obj::<TileMap>();

            UpdateHandler::new(move || {
                let dt = get_frame_time();
                let mut player = player.get_mut();
                let mut player_client = player_client.get_mut();

                // Handle building
                'build: {
                    let mut tile_map = tile_map.get_mut();

                    let mouse_pos = camera_mgr.get_mut().project(mouse_position().into());
                    let layer = tile_map.layer("under_player");
                    let layer_config = tile_map.layer_config(layer);

                    let placed_material = if is_mouse_button_down(MouseButton::Right) {
                        tile_map.materials.get().get_by_name("placeholder")
                    } else if is_mouse_button_down(MouseButton::Left) {
                        tile_map.materials.get().get_by_name("air")
                    } else {
                        player_client.last_build_state = false;
                        break 'build;
                    };

                    if player_client.last_build_state {
                        let mut origin = player_client.last_build_pos;
                        let mut length = (mouse_pos - player_client.last_build_pos).length();
                        let delta = (mouse_pos - player_client.last_build_pos) / length;

                        if !delta.is_nan() {
                            while length > 0. {
                                let step_size = length.min(layer_config.size);
                                for isect in layer_config.step_ray(origin, delta * step_size) {
                                    tile_map.set(layer, isect.entered_tile, placed_material);
                                }
                                length -= step_size;
                                origin += delta * step_size;
                            }
                        }
                    }

                    tile_map.set(
                        layer,
                        layer_config.actor_to_tile(mouse_pos),
                        placed_material,
                    );

                    player_client.last_build_state = true;
                    player_client.last_build_pos = mouse_pos;
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
            let camera = me.obj::<VirtualCamera>();

            RenderHandler::new(move || {
                let xform = xform.get();
                let pos = xform.global_pos();

                // FOV change
                {
                    let mut camera = camera.get_mut();
                    let camera = camera.constraints_mut();
                    camera.keep_area = Some(lerp_f32(
                        camera.keep_area.unwrap(),
                        100. + me.get::<PlayerState>().velocity.x.abs() * 10.,
                        0.05,
                    ));
                }

                // Mouse highlight
                {
                    let mouse_pos = camera_mgr.get_mut().project(mouse_position().into());
                    let tile_map = me.deep_obj::<TileMap>().get();
                    let layer = tile_map.layer("under_player");
                    let aabb = tile_map
                        .tile_to_actor_rect(layer, tile_map.actor_to_tile(layer, mouse_pos));

                    draw_rectangle(aabb.x(), aabb.y(), aabb.w(), aabb.h(), BLUE);
                }

                // Character rendering
                draw_circle(pos.x, pos.y, 0.3, RED);
            })
        })
        .with_cyclic(|me, _| {
            DespawnHandler::new(move || {
                me.get::<Collider>().despawn();
            })
        })
}
