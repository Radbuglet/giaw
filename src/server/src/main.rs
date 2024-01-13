use aunty::StrongEntity;
use giaw_server::net::{
    rpc::{RpcManager, RpcNode, RpcNodeBuilder},
    session::{SessionManager, SessionState},
    transport::{QuadServer, QuadServerEvent},
};
use giaw_shared::game::{
    actors::player::{PlayerPacket1, PlayerRpcs},
    services::{
        rpc::{decode_packet, encode_packet, RpcNodeId, RpcPacket},
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
        .with(RpcManager::default())
        .with(SessionManager::default())
        .with({
            let server = TcpListener::bind("127.0.0.1:8080").await.unwrap();
            QuadServer::new(server)
        })
        .with_cyclic(RpcNode::new(RpcNodeId::ROOT));

    {
        let rpc = root.obj::<RpcNode>();
        let rpc_b = RpcNodeBuilder::new(&rpc);

        rpc_b.sub(PlayerRpcs::Packet1).bind_message({
            let rpc = rpc.clone();
            move |peer, data: PlayerPacket1| {
                rpc.get()
                    .queue_message(peer, PlayerRpcs::Packet1, encode_packet(&data));
                Ok(())
            }
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

                    let errors = root.obj::<RpcManager>().process_packet(peer, &data);
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
            for (peer, packet) in root.get_mut::<RpcManager>().produce_packets() {
                server.send(peer.get::<SessionState>().id, encode_packet(&packet));
            }
        }
    }
}
