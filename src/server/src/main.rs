use std::net::SocketAddr;

use giaw_server::net::transport::QuadNetStream;
use giaw_shared::game::{
    actors::player::{PlayerPacket1, PlayerRpcs},
    services::rpc::{encode_packet, RpcNodeId, RpcPacket, RpcPacketPart, RpcPathBuilder},
};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() {
    // Install logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace")).init();

    // Start server
    let server = TcpListener::bind("127.0.0.1:8080").await.unwrap();

    while let Ok((stream, addr)) = server.accept().await {
        tokio::spawn(async move {
            log::info!("{addr:?}: connected!");

            if let Err(err) = process_stream(stream, addr).await {
                log::info!("{addr:?}: error in stream {err:?}!");
            } else {
                log::info!("{addr:?}: disconnected!");
            }
        });
    }
}

async fn process_stream(stream: TcpStream, addr: SocketAddr) -> anyhow::Result<()> {
    let mut stream = QuadNetStream::new(stream);

    while let Some(packet) = stream.read().await {
        let packet = packet?;
        log::info!("{addr:?}: {packet:?}");

        stream
            .write(&RpcPacket {
                parts: vec![RpcPacketPart {
                    data: encode_packet(&PlayerPacket1 {
                        hello: 42,
                        world: "World!".to_string(),
                    }),
                    is_catchup: false,
                    node_id: RpcNodeId::ROOT.0.get(),
                    sub_id: PlayerRpcs::Packet1.index(),
                }],
                kick: true,
            })
            .await?;
    }

    Ok(())
}
