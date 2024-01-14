use aunty::StrongEntity;
use giaw_server::net::{
    session::{SessionManager, SessionState},
    transport::{QuadServer, QuadServerEvent},
};
use giaw_shared::game::{
    actors::player::{PlayerPacket1, PlayerRpcs},
    services::{
        rpc::{
            decode_packet, encode_packet, RpcManagerServer, RpcNodeBuilder, RpcNodeId,
            RpcNodeServer, RpcPacket,
        },
        transform::Transform,
    },
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // Install backtrace helper
    color_backtrace::install();

    // Install logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace")).init();

    // Create engine root
    let root = StrongEntity::new()
        .with_debug_label("engine root")
        .with_cyclic(Transform::new(None))
        .with(RpcManagerServer::default())
        .with(SessionManager::default())
        .with({
            let server = TcpListener::bind("127.0.0.1:8080").await.unwrap();
            QuadServer::new(server)
        })
        .with_cyclic(RpcNodeServer::new(RpcNodeId::ROOT));

    {
        let rpc = root.obj::<RpcNodeServer>();
        let rpc = RpcNodeBuilder::new(&rpc);

        let packet_1 = rpc.sub(PlayerRpcs::Packet1);
        let packet_2 = rpc.sub(PlayerRpcs::Packet2).sender();

        packet_1.bind_message(move |peer, _, data: PlayerPacket1| {
            packet_2.send(peer, &data);
            Ok(())
        });
    }

    // Start main loop
    loop {
        // Poll for new network events
        let events = root.get_mut::<QuadServer>().poll().unwrap();

        // Handle the packets
        for event in events {
            match event {
                QuadServerEvent::PeerConnected { id, addr } => {
                    log::info!("Socket {id:?} at address {addr:?} connected!");
                    root.get_mut::<SessionManager>().add_peer(id);
                }
                QuadServerEvent::PeerData { id, data } => {
                    log::info!("Socket {id:?} sent {data:?}");

                    let peer = root.get::<SessionManager>().peer_by_id(id);
                    let Ok(data) = decode_packet::<RpcPacket>(&data) else {
                        todo!();
                    };

                    let errors = root.obj::<RpcManagerServer>().process_packet(peer, &data);
                    if !errors.is_empty() {
                        todo!();
                    }
                }
                QuadServerEvent::PeerDisconnect { id, err } => {
                    log::info!("Socket {id:?} disconnected (error: {err:?})!");
                    root.get_mut::<SessionManager>().remove_peer(id);
                }
            }
        }

        // Send RPCs back
        {
            let mut server = root.get_mut::<QuadServer>();
            for (peer, packet) in root.get_mut::<RpcManagerServer>().drain_queues() {
                server.send(peer.get::<SessionState>().id, encode_packet(&packet));
            }
        }
    }
}
