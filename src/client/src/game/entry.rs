use aunty::{Obj, StrongEntity};
use bytes::Bytes;
use giaw_shared::{
    game::services::{
        actors::{ActorManager, DespawnHandler, UpdateHandler},
        kinematic::{KinematicManager, TileColliderDescriptor},
        rpc::{decode_packet, encode_packet, ClientRpcManager, RpcNodeId, RpcPacket},
        tile::{TileLayerConfig, TileMap},
        transform::{ColliderManager, Transform},
    },
    util::math::aabb::{Aabb, AabbI},
};
use macroquad::{color::GREEN, math::IVec2};
use quad_net::quad_socket::client::QuadSocket;

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
        .with(TileMap::default())
        .with(ClientRpcManager::default())
        .with(QuadSocket::connect("127.0.0.1:8080").unwrap())
        .with_cyclic(KinematicManager::new())
        .with_cyclic(WorldRenderer::new())
        .with_cyclic(|me, _| {
            UpdateHandler::new(move || {
                // Process inbound packets
                {
                    let packet = me.get_mut::<QuadSocket>().try_recv();
                    if let Some(packet) = packet {
                        let packet = decode_packet::<RpcPacket>(&Bytes::from(packet)).unwrap();

                        let errors = me.obj::<ClientRpcManager>().process_packet((), &packet);
                        if !errors.is_empty() {
                            panic!("Errors while processing packet {packet:?}: {errors:#?}");
                        }
                    }
                }

                // Update actors
                {
                    let actor_mgr = me.get::<ActorManager>();

                    cbit::cbit!(for actor in actor_mgr.iter_actors() {
                        actor.get::<UpdateHandler>().call();
                    });

                    actor_mgr.process_despawns();
                }

                // Process outbound packets
                {
                    let mut socket = me.get_mut::<QuadSocket>();
                    let mut manager = me.get_mut::<ClientRpcManager>();

                    for ((), packet) in manager.drain_queues() {
                        socket.send(&encode_packet(&packet));
                    }
                }
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

    // Setup initial scene
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

    create_player(&mut scene.get_mut(), RpcNodeId::ROOT, Some(scene.obj()));
    scene
}
