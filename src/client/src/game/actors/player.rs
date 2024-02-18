use std::ops::ControlFlow;

use aunty::{make_extensible, CyclicCtor, Entity, Obj};
use giaw_shared::{
    game::actors::{inventory::InventoryData, player::PlayerState},
    util::{
        game::{
            actors::{ActorManager, DespawnHandler, UpdateHandler},
            rpc::{ClientRpcNode, RpcNodeId},
            tile::{TileLayerConfig, TileMap},
            transform::{Collider, EntityExt, Transform},
        },
        math::{aabb::Aabb, scalar::lerp_f32},
    },
};
use macroquad::{
    color::{BLUE, RED},
    input::{is_key_down, is_key_pressed, is_mouse_button_down, mouse_position},
    math::{IVec2, Vec2},
    miniquad::{KeyCode, MouseButton},
    shapes::{draw_circle, draw_rectangle},
    time::get_frame_time,
};

use crate::{
    engine::scene::RenderHandler,
    game::services::camera::{CameraManager, VirtualCamera, VirtualCameraConstraints},
};

use super::inventory::{ClientItemUseHandler, InteractMode};

// === Components === //

#[derive(Debug, Default)]
pub struct ClientPlayerState {
    last_interact_pos: Vec2,
    last_interact_mode: Option<InteractMode>,
}

#[derive(Debug)]
pub struct ClientPlayerDriver {
    // Component dependencies
    me: Entity,
    xform: Obj<Transform>,
    state: Obj<PlayerState>,
    client_state: Obj<ClientPlayerState>,
    camera: Obj<VirtualCamera>,
    inventory: Obj<InventoryData>,

    // Deep dependencies
    camera_mgr: Obj<CameraManager>,
    tile_map: Obj<TileMap>,
}

make_extensible!(pub ClientPlayerDriverObj for ClientPlayerDriver);

impl ClientPlayerDriver {
    pub fn new() -> impl CyclicCtor<Self> {
        |me, _| Self {
            me,
            xform: me.obj(),
            state: me.obj(),
            client_state: me.obj(),
            camera: me.obj(),
            inventory: me.obj(),
            camera_mgr: me.deep_obj(),
            tile_map: me.deep_obj(),
        }
    }

    pub fn update(&self) {
        let dt = get_frame_time();

        // Handle inventory selection
        {
            let mut player = self.state.get_mut();

            let keys = [
                KeyCode::Key1,
                KeyCode::Key2,
                KeyCode::Key3,
                KeyCode::Key4,
                KeyCode::Key5,
                KeyCode::Key6,
                KeyCode::Key7,
                KeyCode::Key8,
                KeyCode::Key9,
            ];

            for (i, key) in keys.into_iter().enumerate() {
                if is_key_pressed(key) {
                    player.hotbar_slot = i;
                }
            }
        }

        // Handle interactions
        'interact: {
            let mut player_client = self.client_state.get_mut();

            // Determine interaction mode
            let mode = if is_mouse_button_down(MouseButton::Left) {
                InteractMode::Break
            } else if is_mouse_button_down(MouseButton::Right) {
                InteractMode::Build
            } else {
                player_client.last_interact_mode = None;
                drop(player_client); // (for AuToken)
                break 'interact;
            };

            // Determine current world-space mouse position
            let curr_pos = self.camera_mgr.get_mut().project(mouse_position().into());

            // Determine last world-space mouse position if applicable
            let last_pos = if player_client.last_interact_mode == Some(mode) {
                player_client.last_interact_pos
            } else {
                curr_pos
            };

            // Update interaction state
            player_client.last_interact_pos = curr_pos;
            player_client.last_interact_mode = Some(mode);

            // Call out to inventory
            drop(player_client);
            let hotbar_slot = self.state.get().hotbar_slot;
            let Some((item, item_material)) = self.inventory.get().stacks()[hotbar_slot]
                .as_ref()
                .map(|item| (item.clone(), item.get().material))
            else {
                break 'interact;
            };

            item_material
                .get::<ClientItemUseHandler>()
                .call(self.me, item, mode, last_pos, curr_pos);
        }

        // Handle motion
        {
            let mut player = self.state.get_mut();
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
    }

    pub fn render(&self) {
        let xform = self.xform.get();
        let pos = xform.global_pos();

        // FOV change
        {
            let mut camera = self.camera.get_mut();
            let camera = camera.constraints_mut();
            camera.keep_area = Some(lerp_f32(
                camera.keep_area.unwrap(),
                100. + self.state.get().velocity.x.abs() * 10.,
                0.05,
            ));
        }

        // Mouse highlight
        {
            let mouse_pos = self.camera_mgr.get_mut().project(mouse_position().into());
            let tile_map = self.tile_map.get();
            let layer = tile_map.layer("under_player");
            let aabb = tile_map.tile_to_actor_rect(layer, tile_map.actor_to_tile(layer, mouse_pos));

            draw_rectangle(aabb.x(), aabb.y(), aabb.w(), aabb.h(), BLUE);
        }

        // Character rendering
        draw_circle(pos.x, pos.y, 0.3, RED);
    }

    pub fn selected_tiles<B>(
        &self,
        config: TileLayerConfig,
        src: Vec2,
        dst: Vec2,
        mut f: impl FnMut(IVec2) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        let mut origin = src;
        let mut length = (dst - src).length();
        let delta = (src - dst) / length;

        if !delta.is_nan() {
            while length > 0. {
                let step_size = length.min(config.size);
                for isect in config.step_ray(origin, delta * step_size) {
                    f(isect.entered_tile)?;
                }
                length -= step_size;
                origin += delta * step_size;
            }
        }

        f(config.actor_to_tile(dst))?;

        ControlFlow::Continue(())
    }
}

impl ClientPlayerDriverObj {
    pub fn updater(&self) -> UpdateHandler {
        let me = self.obj.clone();
        UpdateHandler::new(move || me.get().update())
    }

    pub fn renderer(&self) -> RenderHandler {
        let me = self.obj.clone();
        RenderHandler::new(move || me.get().render())
    }
}

// === Prefabs === //

pub fn create_player(
    actors: &ActorManager,
    rpc_id: RpcNodeId,
    parent: Option<Obj<Transform>>,
) -> Entity {
    actors
        .spawn()
        .with_debug_label("player")
        .with_cyclic(Transform::new(parent))
        .with_cyclic(Collider::new_centered(Vec2::ZERO, Vec2::splat(0.6)))
        .with_cyclic(ClientRpcNode::new(rpc_id))
        .with_cyclic(InventoryData::new(9 * 4))
        .with_cyclic(VirtualCamera::new_attached(
            Aabb::ZERO,
            VirtualCameraConstraints::default().keep_visible_area(Vec2::splat(10.)),
        ))
        .with_cyclic(PlayerState::new())
        .with(ClientPlayerState::default())
        .with_cyclic(ClientPlayerDriver::new())
        // Handlers
        .with_cyclic(|me, _| me.obj::<ClientPlayerDriver>().updater())
        .with_cyclic(|me, _| me.obj::<ClientPlayerDriver>().renderer())
        .with_cyclic(|me, _| {
            DespawnHandler::new(move || {
                me.get::<Collider>().despawn();
                me.get::<ClientRpcNode>().despawn();
            })
        })
}
