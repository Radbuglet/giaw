use std::{collections::HashMap, net::SocketAddr};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures::SinkExt;
use tokio::{
    net::TcpListener,
    sync::mpsc::{channel, error::TryRecvError, unbounded_channel, Receiver, UnboundedSender},
};
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, Encoder, Framed};

// === Server === //

const SERVER_EVENT_CHANNEL_SIZE: usize = 16;

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct QuadPeerId(u64);

#[derive(Debug)]
pub enum QuadServerEvent {
    PeerConnected {
        id: QuadPeerId,
        addr: SocketAddr,
    },
    PeerData {
        id: QuadPeerId,
        data: Bytes,
    },
    PeerDisconnect {
        id: QuadPeerId,
        err: Option<anyhow::Error>,
    },
}

// QuadServer
#[derive(Debug)]
pub struct QuadServer {
    events: Receiver<InternalServerEvent>,
    sockets: HashMap<QuadPeerId, SocketState>,
}

enum InternalServerEvent {
    PeerConnected {
        id: QuadPeerId,
        state: SocketState,
    },
    PeerData {
        id: QuadPeerId,
        data: Bytes,
    },
    PeerDisconnect {
        id: QuadPeerId,
        err: Option<anyhow::Error>,
    },
    ServerError(anyhow::Error),
}

#[derive(Debug)]
struct SocketState {
    addr: SocketAddr,
    sender: UnboundedSender<Bytes>,
}

impl QuadServer {
    pub fn new(listener: TcpListener) -> Self {
        let (server_send, server_recv) = channel(SERVER_EVENT_CHANNEL_SIZE);

        tokio::spawn(async move {
            let mut id_gen = 0u64;

            loop {
                // Wait for either a peer to connect, for the pipe to be broken, or the `QuadServer`
                // to be dropped.
                let (stream, addr) = tokio::select! {
                    peer = listener.accept() => match peer {
                        Ok(peer) => peer,
                        Err(err) => {
                            let _ = server_send
                                .send(InternalServerEvent::ServerError(anyhow::Error::new(err)))
                                .await;

                            break;
                        },
                    },
                    // If it was dropped, the server should shut-down.
                    _ = server_send.closed() => break,
                };

                // Initialize state for the socket
                let mut stream = Framed::new(stream, QuadNetCodec);
                let (socket_send, mut socket_recv) = unbounded_channel();
                let id = QuadPeerId(id_gen);
                let server_send = server_send.clone();
                id_gen += 1;

                // Notify the main thread of its existence
                let _ = server_send
                    .send(InternalServerEvent::PeerConnected {
                        id,
                        state: SocketState {
                            addr,
                            sender: socket_send,
                        },
                    })
                    .await;

                // Spin up a thread to process its packets
                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            // A network client wants us to do something.
                            ev = stream.next() => {
                                match ev {
                                    // We received a packet.
                                    Some(Ok(data)) => {
                                        let _ = server_send.send(InternalServerEvent::PeerData { id, data }).await;
                                    },

                                    // We failed to poll the socket.
                                    Some(Err(err)) => {
                                        // Notify the main thread...
                                        let _ = server_send.send(
                                            InternalServerEvent::PeerDisconnect {
                                                id,
                                                err: Some(err),
                                            },
                                        ).await;

                                        // And close the socket.
                                        break;
                                    },

                                    // The socket closed naturally
                                    None => {
                                        // Notify the main thread...
                                        let _ = server_send.send(
                                            InternalServerEvent::PeerDisconnect {
                                                id,
                                                err: None,
                                            },
                                        ).await;

                                        // And close the socket.
                                        break;
                                    },
                                }
                            },

                            // The main thread wants us to do something.
                            ev = socket_recv.recv() => {
                                let Some(ev) = ev else {
                                    // The main thread wants this client kicked.
                                    break
                                };

                                if let Err(err) = stream.send(&ev).await {
                                    // A fatal ocurred while trying to communicate with this peer.
                                    // Notify the main thread...
                                    let _ = server_send.send(
                                        InternalServerEvent::PeerDisconnect {
                                            id,
                                            err: Some(err),
                                        },
                                    ).await;

                                    // And close the socket.
                                    break;
                                }
                            },
                        }
                    }

                    drop(stream);
                });
            }

            drop(listener);
        });

        Self {
            events: server_recv,
            sockets: HashMap::default(),
        }
    }

    pub fn poll(&mut self) -> anyhow::Result<Vec<QuadServerEvent>> {
        let mut events = Vec::new();

        loop {
            let event = match self.events.try_recv() {
                Ok(ev) => ev,
                Err(TryRecvError::Disconnected) => unreachable!(),
                Err(TryRecvError::Empty) => break,
            };

            match event {
                InternalServerEvent::PeerConnected { id, state } => {
                    events.push(QuadServerEvent::PeerConnected {
                        id,
                        addr: state.addr,
                    });

                    self.sockets.insert(id, state);
                }
                InternalServerEvent::PeerData { id, data } => {
                    events.push(QuadServerEvent::PeerData { id, data });
                }
                InternalServerEvent::PeerDisconnect { id, err } => {
                    events.push(QuadServerEvent::PeerDisconnect { id, err });
                    self.sockets.remove(&id);
                }
                InternalServerEvent::ServerError(err) => return Err(err),
            }
        }

        Ok(events)
    }

    pub fn send(&mut self, id: QuadPeerId, data: Bytes) {
        if let Some(socket) = self.sockets.get(&id) {
            let _ = socket.sender.send(data);
        }
    }
}

// === Framing === //

struct QuadNetCodec;

impl Decoder for QuadNetCodec {
    type Item = Bytes;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let Some(packet_len) = src.first().map(|v| *v as usize) else {
            return Ok(None);
        };

        if src.len() <= packet_len {
            return Ok(None);
        }

        let packet = src.clone().freeze().slice(1..).slice(..packet_len);
        src.advance(packet_len + 1);

        Ok(Some(packet))
    }
}

impl<'a> Encoder<&'a [u8]> for QuadNetCodec {
    type Error = anyhow::Error;

    fn encode(&mut self, item: &'a [u8], dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.put_u8(u8::try_from(item.len()).unwrap());
        dst.put(item);
        Ok(())
    }
}
