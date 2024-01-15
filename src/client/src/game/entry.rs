use aunty::{autoken::ImmutableBorrow, make_extensible, CyclicCtor, Entity, Obj, StrongEntity};
use bytes::Bytes;
use giaw_shared::{
    game::{
        actors::{
            inventory::{InventoryData, ItemRegistry, ItemStackBase},
            player::PlayerState,
        },
        services::{
            actors::{ActorManager, DespawnHandler, UpdateHandler},
            kinematic::{KinematicManager, TileColliderDescriptor},
            rpc::{decode_packet, encode_packet, ClientRpcManager, RpcNodeId, RpcPacket},
            tile::{TileLayerConfig, TileMap},
            transform::{ColliderManager, EntityExt, Transform},
        },
    },
    util::math::aabb::{Aabb, AabbI},
};
use macroquad::{
    color::{BLACK, GRAY, GREEN, WHITE},
    math::{IVec2, Vec2},
    shapes::draw_rectangle,
};
use quad_net::quad_socket::client::QuadSocket;

use crate::{engine::scene::RenderHandler, game::actors::inventory::InteractMode};

use super::{
    actors::{
        inventory::{ClientItemDescriptor, ClientItemUseHandler},
        player::{create_player, ClientPlayerDriver},
    },
    services::{
        camera::CameraManager,
        render::{TileVisualDescriptor, WorldRenderer},
    },
};

// === Components === //

#[derive(Debug, Default)]
pub struct GameClientState {
    local_player: Option<Entity>,
}

pub struct GameClientDriver {
    // Networking
    socket: Obj<QuadSocket>,
    rpc_manager: Obj<ClientRpcManager>,

    // Game
    actors: Obj<ActorManager>,
    state: Obj<GameClientState>,
    renderer: Obj<WorldRenderer>,
}

make_extensible!(pub GameClientDriverObj for GameClientDriver);

impl GameClientDriver {
    pub fn new() -> impl CyclicCtor<Self> {
        move |me, _| Self {
            socket: me.obj(),
            rpc_manager: me.obj(),
            actors: me.obj(),
            state: me.obj(),
            renderer: me.obj(),
        }
    }

    pub fn update(&self) {
        // Process inbound packets
        {
            let packet = self.socket.get_mut().try_recv();
            if let Some(packet) = packet {
                let packet = decode_packet::<RpcPacket>(&Bytes::from(packet)).unwrap();

                let errors = self.rpc_manager.process_packet((), &packet);
                if !errors.is_empty() {
                    panic!("Errors while processing packet {packet:?}: {errors:#?}");
                }
            }
        }

        // Update actors
        {
            let actor_mgr = self.actors.get();

            cbit::cbit!(for actor in actor_mgr.iter_actors() {
                let loaner = ImmutableBorrow::new();
                if let Some(handler) = actor.try_get::<UpdateHandler>(&loaner) {
                    handler.call();
                };
            });

            actor_mgr.process_despawns();
        }

        // Process outbound packets
        {
            let mut socket = self.socket.get_mut();
            let mut manager = self.rpc_manager.get_mut();

            for ((), packet) in manager.drain_queues() {
                socket.send(&encode_packet(&packet));
            }
        }
    }

    pub fn render(&self) {
        // Render world
        self.renderer.get().render();

        // Render UI
        if let Some(player) = self.state.get().local_player {
            let inventory = player.get::<InventoryData>();
            let selected = player.get::<PlayerState>().hotbar_slot;

            for (i, item) in inventory.stacks()[0..9].iter().enumerate() {
                let item_aabb =
                    Aabb::new(10., 10., 50., 50.).translated(Vec2::new(i as f32 * 60., 0.));

                if selected == i {
                    let aabb = item_aabb.grow(Vec2::splat(5.));
                    draw_rectangle(aabb.x(), aabb.y(), aabb.w(), aabb.h(), BLACK);

                    let aabb = item_aabb.grow(Vec2::splat(3.));
                    draw_rectangle(aabb.x(), aabb.y(), aabb.w(), aabb.h(), WHITE);
                }

                let Some(item) = item else { continue };
                let item = item.get();
                let item_descriptor = item.material.get::<ClientItemDescriptor>();

                draw_rectangle(
                    item_aabb.x(),
                    item_aabb.y(),
                    item_aabb.w(),
                    item_aabb.h(),
                    item_descriptor.color,
                );
            }
        }
    }
}

impl GameClientDriverObj {
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

pub fn create_game(parent: Option<Obj<Transform>>) -> StrongEntity {
    let scene = StrongEntity::new()
        .with_debug_label("game scene root")
        // Attach core services
        .with_cyclic(Transform::new(parent))
        .with(ActorManager::default())
        .with(ColliderManager::default())
        .with(CameraManager::default())
        .with(TileMap::default())
        .with_cyclic(KinematicManager::new())
        .with_cyclic(WorldRenderer::new())
        // Attach game services
        .with(ItemRegistry::default())
        // Attach networking services
        .with(ClientRpcManager::default())
        .with(QuadSocket::connect("127.0.0.1:8080").unwrap())
        // Attach scene entrypoints
        .with(GameClientState::default())
        .with_cyclic(GameClientDriver::new())
        .with_cyclic(|me, _| me.obj::<GameClientDriver>().updater())
        .with_cyclic(|me, _| me.obj::<GameClientDriver>().renderer())
        .with_cyclic(|me, _| {
            DespawnHandler::new(move || {
                me.get::<ActorManager>().despawn_all();
            })
        });

    // Setup initial scene
    {
        // Setup basic map
        {
            let mut map = scene.get_mut::<TileMap>();
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
        }

        // Setup player
        {
            let actors = scene.get::<ActorManager>();
            let mut item_registry = scene.get_mut::<ItemRegistry>();

            let stone = item_registry.register(
                "stone",
                StrongEntity::new()
                    .with_debug_label("stone")
                    .with(ClientItemDescriptor { color: GRAY })
                    .with(ClientItemUseHandler::new(
                        |player, _stack, mode, from, to| {
                            let mut tile_map = player.deep_obj::<TileMap>().get_mut();
                            let layer = tile_map.layer("under_player");
                            let layer_config = tile_map.layer_config(layer);
                            let material = match mode {
                                InteractMode::Build => {
                                    tile_map.materials.get().get_by_name("placeholder")
                                }
                                InteractMode::Break => tile_map.materials.get().get_by_name("air"),
                            };

                            let player = player.get::<ClientPlayerDriver>();
                            cbit::cbit!(for pos in player.selected_tiles(layer_config, from, to) {
                                tile_map.set(layer, pos, material);
                            });
                        },
                    )),
            );

            let player = create_player(&actors, RpcNodeId::ROOT, Some(scene.obj()));
            player.get_mut::<InventoryData>().insert_stack(
                actors
                    .spawn()
                    .with_debug_label("my stack")
                    .with_cyclic(Transform::new(Some(player.obj())))
                    .with_cyclic(ItemStackBase::new(stone, 1))
                    .obj(),
            );
            scene.get_mut::<GameClientState>().local_player = Some(player);
        }
    }

    scene
}
