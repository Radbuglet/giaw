use std::num::NonZeroU64;

use aunty::{delegate, make_extensible, CyclicCtor, Entity, Obj};
use bytes::Bytes;
use giaw_shared::{
    game::services::{
        actors::DespawnStep,
        rpc::{
            decode_packet, encode_packet, EmptyPathBuilder, RpcNodeId, RpcPacket, RpcPacketPart,
            RpcPathBuilder,
        },
        transform::EntityExt,
    },
    util::lang::vec::ensure_index,
};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

// === RpcManager === //

#[derive(Debug, Default)]
pub struct RpcManager {
    errors: Vec<anyhow::Error>,
    nodes: FxHashMap<RpcNodeId, Obj<RpcNode>>,
    peer_queues: FxHashMap<Entity, RpcPeerQueue>,
}

#[derive(Debug, Default)]
struct RpcPeerQueue {
    catchups: FxHashMap<(RpcNodeId, u32), Bytes>,
    messages: Vec<RpcPacketPart>,
}

make_extensible!(pub RpcManagerObj for RpcManager);

impl RpcManager {
    pub fn report_error(&mut self, error: anyhow::Error) {
        self.errors.push(error);
    }

    pub fn queue_message(&mut self, peer: Entity, node: RpcNodeId, path: u32, data: Bytes) {
        self.peer_queues
            .entry(peer)
            .or_default()
            .messages
            .push(RpcPacketPart {
                node_id: node.0.get(),
                path,
                data,
            });
    }

    pub fn produce_packets(&mut self) -> impl Iterator<Item = (Entity, RpcPacket)> + '_ {
        self.peer_queues.drain().map(|(peer, queue)| {
            (
                peer,
                RpcPacket {
                    catchup: queue
                        .catchups
                        .into_iter()
                        .map(|((node, path), data)| RpcPacketPart {
                            node_id: node.0.get(),
                            path,
                            data,
                        })
                        .collect(),
                    messages: queue.messages,
                },
            )
        })
    }
}

impl RpcManagerObj {
    #[must_use]
    pub fn process_packet(&self, peer: Entity, packet: &RpcPacket) -> Vec<anyhow::Error> {
        // Process message packets
        for part in &packet.messages {
            let Some(id) = NonZeroU64::new(part.node_id).map(RpcNodeId) else {
                self.report_error(anyhow::anyhow!("encountered invalid null node ID"));
                continue;
            };

            let Some(target) = self.obj.get().nodes.get(&id).cloned() else {
                self.report_error(anyhow::anyhow!(
                    "attempted to send RPC to unknown node {id:?}"
                ));
                continue;
            };

            let Some(handler) = target
                .get()
                .message_handlers
                .get(part.path as usize)
                .cloned()
                .flatten()
            else {
                self.report_error(anyhow::anyhow!(
                    "attempted to send RPC to unknown path {:?} on node {:?} with id {id:?}",
                    part.path,
                    target,
                ));
                continue;
            };

            handler.call(peer, &part.data);
        }

        // Reject catchup packets
        if !packet.catchup.is_empty() {
            self.report_error(anyhow::anyhow!("client attempted to send catchup packet"));
        }

        // Report errors
        std::mem::take(&mut self.obj.get_mut().errors)
    }

    pub fn queue_message(&self, peer: Entity, node: RpcNodeId, path: u32, data: Bytes) {
        self.obj.get_mut().queue_message(peer, node, path, data)
    }

    pub fn queue_catchup(
        &self,
        peer: Entity,
        node: RpcNodeId,
        path: u32,
        data: impl FnOnce() -> Bytes,
    ) {
        // Skip catchups which have already been generated.
        if !self
            .obj
            .get_mut()
            // Get or create peer queue
            .peer_queues
            .entry(peer)
            .or_default()
            // See if we've generated the catchup yet.
            .catchups
            .contains_key(&(node, path))
        {
            return;
        }

        // Generate the data
        let data = data();

        // ...and insert it!
        self.obj
            .get_mut()
            .peer_queues
            .get_mut(&peer)
            .unwrap()
            .catchups
            .insert((node, path), data);
    }

    pub fn report_error(&self, error: anyhow::Error) {
        self.obj.get_mut().report_error(error);
    }

    pub fn catch_errors<R>(&self, f: impl FnOnce() -> R) -> Result<R, R> {
        let old_len = self.obj.get().errors.len();
        let res = f();

        let new_len = self.obj.get().errors.len();
        if new_len == old_len {
            Ok(res)
        } else {
            Err(res)
        }
    }
}

// === RpcNode === //

delegate! {
    pub fn RpcMessageHandler(peer: Entity, data: &Bytes)
}

delegate! {
    pub fn RpcCatchupHandler(peer: Entity) -> Bytes
}

#[derive(Debug)]
pub struct RpcNode {
    despawn: DespawnStep,
    me: Entity,
    id: RpcNodeId,
    manager: Obj<RpcManager>,
    message_handlers: Vec<Option<RpcMessageHandler>>,
    catchup_handlers: Vec<(u32, RpcCatchupHandler)>,
}

make_extensible!(pub RpcNodeObj for RpcNode);

impl RpcNode {
    pub fn new(id: RpcNodeId) -> impl CyclicCtor<Self> {
        move |me, ob| {
            let manager = me.deep_obj::<RpcManager>();

            manager.get_mut().nodes.insert(id, ob.clone());

            Self {
                despawn: DespawnStep::default(),
                me,
                id,
                manager,
                message_handlers: Vec::new(),
                catchup_handlers: Vec::new(),
            }
        }
    }

    pub fn id(&self) -> RpcNodeId {
        self.id
    }

    pub fn manager(&self) -> &Obj<RpcManager> {
        &self.manager
    }

    pub fn entity(&self) -> Entity {
        self.me
    }

    pub fn report_error_direct(&self, error: anyhow::Error) {
        self.manager.get_mut().report_error(error);
    }

    pub fn bind_message_handler(&mut self, path: impl RpcPathBuilder, handler: RpcMessageHandler) {
        let slot = ensure_index(&mut self.message_handlers, path.index() as usize);
        debug_assert!(slot.is_none());

        *slot = Some(handler);
    }

    pub fn bind_catchup_handler(&mut self, path: impl RpcPathBuilder, handler: RpcCatchupHandler) {
        self.catchup_handlers.push((path.index(), handler));
    }

    pub fn queue_message(&self, peer: Entity, path: impl RpcPathBuilder, data: Bytes) {
        self.manager
            .queue_message(peer, self.id, path.index(), data);
    }

    pub fn despawn(&self) {
        self.despawn.mark();
        self.manager.get_mut().nodes.remove(&self.id);
    }
}

impl RpcNodeObj {
    pub fn queue_catchup(&self, peer: Entity) {
        let (manager, id, handlers) = {
            let me = self.obj.get();
            (me.manager.clone(), me.id, me.catchup_handlers.clone())
        };

        for (path, handler) in handlers {
            manager.queue_catchup(peer, id, path, || handler.call(peer));
        }
    }
}

// === RpcNodeBuilder === //

#[derive(Debug, Copy, Clone)]
pub struct RpcNodeBuilder<'a, P> {
    pub node: &'a Obj<RpcNode>,
    pub path: P,
}

impl<'a> RpcNodeBuilder<'a, EmptyPathBuilder> {
    pub fn new(node: &'a Obj<RpcNode>) -> Self {
        Self {
            node,
            path: EmptyPathBuilder,
        }
    }
}

impl<'a, P> RpcNodeBuilder<'a, P> {
    #[allow(clippy::should_implement_trait)]
    pub fn sub<C, R1, R2>(self, part: C) -> RpcNodeBuilder<'a, (P, C)>
    where
        P: RpcPathBuilder<R1>,
        C: RpcPathBuilder<R2, Output = R1>,
    {
        RpcNodeBuilder {
            node: self.node,
            path: self.path.sub(part),
        }
    }
}

impl<P: RpcPathBuilder> RpcNodeBuilder<'_, P> {
    pub fn bind_message_raw(self, handler: impl 'static + Fn(Entity, &Bytes) -> anyhow::Result<()>)
    where
        P: 'static,
    {
        let me = (self.node.clone(), self.path);

        self.node.get_mut().bind_message_handler(
            self.path,
            RpcMessageHandler::new(move |peer, data| {
                let me = RpcNodeBuilder {
                    node: &me.0,
                    path: me.1,
                };

                me.handle_error(|_| handler(peer, data));
            }),
        )
    }

    pub fn bind_message<D>(self, handler: impl 'static + Fn(Entity, D) -> anyhow::Result<()>)
    where
        P: 'static,
        D: for<'a> Deserialize<'a>,
    {
        self.bind_message_raw(move |peer, data| handler(peer, decode_packet::<D>(data)?));
    }

    pub fn bind_catchup_raw(self, handler: impl 'static + Fn(Entity) -> Bytes)
    where
        P: 'static,
    {
        self.node
            .get_mut()
            .bind_catchup_handler(self.path, RpcCatchupHandler::new(handler))
    }

    pub fn bind_catchup<D: Serialize>(self, handler: impl 'static + Fn(Entity) -> D)
    where
        P: 'static,
    {
        self.node.get_mut().bind_catchup_handler(
            self.path,
            RpcCatchupHandler::new(move |peer| encode_packet(&handler(peer))),
        )
    }

    pub fn report_error(self, error: anyhow::Error) {
        self.node.get().report_error_direct(error.context(format!(
            "failed to process RPC on node {:?} and path {:?}",
            self.node.get().entity(),
            self.path.make(()),
        )))
    }

    pub fn handle_error<R>(self, f: impl FnOnce(Self) -> anyhow::Result<R>) -> Option<R> {
        match f(self) {
            Ok(v) => Some(v),
            Err(e) => {
                self.report_error(e);
                None
            }
        }
    }
}
