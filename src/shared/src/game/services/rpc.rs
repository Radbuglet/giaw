use std::{fmt, hash, marker::PhantomData, num::NonZeroU64};

use aunty::{delegate, make_extensible, CyclicCtor, Entity, Obj};
use bytes::Bytes;
use derive_where::derive_where;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

// === Path === //

// Builder
pub trait CompleteRpcPath: fmt::Debug {
    const MAX: u32;

    fn as_index(&self) -> u32;
}

impl CompleteRpcPath for () {
    const MAX: u32 = 1;

    fn as_index(&self) -> u32 {
        0
    }
}

pub trait RpcPath<R = ()>: Copy {
    type Output: CompleteRpcPath;

    fn index(self) -> u32
    where
        R: Default,
    {
        self.make(R::default()).as_index()
    }

    fn make(self, remainder: R) -> Self::Output;

    fn sub<C, R2>(self, part: C) -> (Self, C)
    where
        C: RpcPath<R2, Output = R>,
    {
        (self, part)
    }
}

impl<F, R, O> RpcPath<R> for F
where
    O: CompleteRpcPath,
    F: Copy + FnOnce(R) -> O,
{
    type Output = O;

    fn make(self, value: R) -> Self::Output {
        self(value)
    }
}

impl<R, A, B> RpcPath<R> for (A, B)
where
    A: RpcPath<B::Output>,
    B: RpcPath<R>,
{
    type Output = A::Output;

    fn make(self, remainder: R) -> Self::Output {
        self.0.make(self.1.make(remainder))
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct EmptyPathBuilder;

impl<R: CompleteRpcPath> RpcPath<R> for EmptyPathBuilder {
    type Output = R;

    fn make(self, remainder: R) -> Self::Output {
        remainder
    }
}

// Macro
#[doc(hidden)]
pub mod rpc_path_macro_internals {
    pub use {
        super::CompleteRpcPath,
        std::{primitive::u32, unreachable},
    };
}

#[macro_export]
macro_rules! rpc_path {
    ($(
        $(#[$attr:meta])*
        $vis:vis enum $enum_name:ident {
            $(
                $variant_name:ident$(($variant_ty:ty))?
            ),*$(,)?
        }
    )*) => {$(
        $(#[$attr])*
        #[allow(unused_parens)]
        #[derive(Debug, Copy, Clone)]
        $vis enum $enum_name {
            $($variant_name(( $( $variant_ty )? ))),*
        }

        #[allow(irrefutable_let_patterns, unused_variables, unused_parens)]
        impl $crate::game::services::rpc::rpc_path_macro_internals::CompleteRpcPath for $enum_name {
            const MAX: $crate::game::services::rpc::rpc_path_macro_internals::u32 = 0
                $(+ <($($variant_ty)?) as $crate::game::services::rpc::rpc_path_macro_internals::CompleteRpcPath>::MAX)*;

            fn as_index(&self) -> $crate::game::services::rpc::rpc_path_macro_internals::u32 {
                let offset = 0;

                $(
                    if let Self::$variant_name(var) = self {
                        return offset + $crate::game::services::rpc::rpc_path_macro_internals::CompleteRpcPath::as_index(var);
                    }

                    let offset = offset + <($($variant_ty)?) as $crate::game::services::rpc::rpc_path_macro_internals::CompleteRpcPath>::MAX;
                )*

                $crate::game::services::rpc::rpc_path_macro_internals::unreachable!();
            }
        }
    )*};
}

pub use rpc_path;

use crate::util::lang::vec::ensure_index;

use super::{actors::DespawnStep, transform::EntityExt};

// === Protocol === //

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcPacket {
    pub catchup: Vec<RpcPacketMessage>,
    pub messages: Vec<RpcPacketMessage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcPacketMessage {
    pub node_id: u64,
    pub path: u32,
    pub data: Bytes,
}

pub fn encode_packet(v: &impl Serialize) -> Bytes {
    Bytes::from(bincode::serialize(v).unwrap())
}

pub fn decode_packet<'a, P: Deserialize<'a>>(v: &'a Bytes) -> anyhow::Result<P> {
    bincode::deserialize(v).map_err(anyhow::Error::new)
}

// === RpcNodeId === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct RpcNodeId(pub NonZeroU64);

impl RpcNodeId {
    pub const ROOT: Self = RpcNodeId(match NonZeroU64::new(1) {
        Some(v) => v,
        None => unreachable!(),
    });
}

// === NetMode === //

mod sealed {
    pub trait NetModeSealed {}
}

use sealed::NetModeSealed;

pub trait NetMode: 'static + Sized + NetModeSealed {}

#[non_exhaustive]
pub struct ServerNetMode;

impl NetMode for ServerNetMode {}
impl NetModeSealed for ServerNetMode {}

#[non_exhaustive]
pub struct ClientNetMode;

impl NetMode for ClientNetMode {}
impl NetModeSealed for ClientNetMode {}

// === RpcManager === //

// Specializations
pub type RpcManagerServer = RpcManager<ServerNetMode>;
pub type RpcManagerClient = RpcManager<ClientNetMode>;

pub trait RpcNetMode: NetMode {
    type Peer: fmt::Debug + hash::Hash + Eq + Copy;
    type QueueCatchupState: fmt::Debug + Default;
    type ManagerCatchupState: fmt::Debug + Default;
    type NodeCatchupState: fmt::Debug + Default;

    fn import_catchup_packets(
        state: &mut Self::ManagerCatchupState,
        packets: &[RpcPacketMessage],
    ) -> anyhow::Result<()>;

    fn clear_catchup_packets(state: &mut Self::ManagerCatchupState);

    fn produce_catchup_packets(state: Self::QueueCatchupState) -> Vec<RpcPacketMessage>;
}

impl RpcNetMode for ServerNetMode {
    type Peer = Entity;
    type QueueCatchupState = FxHashMap<RpcNodeId, Vec<(u32, Bytes)>>;
    type ManagerCatchupState = ();
    type NodeCatchupState = Vec<(u32, RpcCatchupGenerator)>;

    fn import_catchup_packets(
        _state: &mut Self::ManagerCatchupState,
        packets: &[RpcPacketMessage],
    ) -> anyhow::Result<()> {
        if !packets.is_empty() {
            anyhow::bail!("peer somehow sent a catchup packet to the server");
        }

        Ok(())
    }

    fn clear_catchup_packets(_state: &mut Self::ManagerCatchupState) {
        // nothing to clear
    }

    fn produce_catchup_packets(catchups: Self::QueueCatchupState) -> Vec<RpcPacketMessage> {
        catchups
            .into_iter()
            .flat_map(|(node_id, packets)| {
                packets
                    .into_iter()
                    .map(move |(path, data)| RpcPacketMessage {
                        node_id: node_id.0.get(),
                        path,
                        data,
                    })
            })
            .collect()
    }
}

impl RpcNetMode for ClientNetMode {
    type Peer = ();
    type QueueCatchupState = ();
    type ManagerCatchupState = FxHashMap<(RpcNodeId, u32), Bytes>;
    type NodeCatchupState = ();

    fn import_catchup_packets(
        state: &mut Self::ManagerCatchupState,
        packets: &[RpcPacketMessage],
    ) -> anyhow::Result<()> {
        for packet in packets {
            let Some(node_id) = NonZeroU64::new(packet.node_id).map(RpcNodeId) else {
                anyhow::bail!("encountered a catchup packet with a target node ID of 0");
            };
            state.insert((node_id, packet.path), packet.data.clone());
        }

        Ok(())
    }

    fn clear_catchup_packets(state: &mut Self::ManagerCatchupState) {
        state.retain(|_peer, queue| {
            let was_empty = queue.is_empty();
            queue.clear();
            !was_empty
        });
    }

    fn produce_catchup_packets(_state: Self::QueueCatchupState) -> Vec<RpcPacketMessage> {
        vec![]
    }
}

// Core
delegate! {
    pub fn RpcMessageHandler<P>(peer: P, node: Entity, data: &Bytes) -> anyhow::Result<()>
}

delegate! {
    pub fn RpcCatchupGenerator(peer: Entity, node: Entity) -> Bytes
}

#[derive_where(Debug, Default)]
pub struct RpcManager<M: RpcNetMode> {
    _ty: PhantomData<M>,
    nodes: FxHashMap<RpcNodeId, Obj<RpcNode<M>>>,
    packet_queues: FxHashMap<M::Peer, PeerPacketQueue<M>>,
    catchup_state: M::ManagerCatchupState,
}

#[derive_where(Debug, Default)]
struct PeerPacketQueue<M: RpcNetMode> {
    messages: Vec<RpcPacketMessage>,
    catchups: M::QueueCatchupState,
}

make_extensible!(pub RpcManagerObj<M> for RpcManager where M: RpcNetMode);

impl<M: RpcNetMode> RpcManager<M> {
    fn packet_queue(&mut self, peer: M::Peer) -> &mut PeerPacketQueue<M> {
        self.packet_queues.entry(peer).or_default()
    }

    pub fn queue_message(&mut self, peer: M::Peer, node: RpcNodeId, path: u32, data: Bytes) {
        self.packet_queue(peer).messages.push(RpcPacketMessage {
            node_id: node.0.get(),
            path,
            data,
        });
    }

    pub fn drain_queues(&mut self) -> impl Iterator<Item = (M::Peer, RpcPacket)> + '_ {
        self.packet_queues.drain().map(|(peer, queue)| {
            (
                peer,
                RpcPacket {
                    messages: queue.messages,
                    catchup: M::produce_catchup_packets(queue.catchups),
                },
            )
        })
    }
}

impl<M: RpcNetMode> RpcManagerObj<M> {
    #[must_use]
    pub fn process_packet(&self, peer: M::Peer, packet: &RpcPacket) -> Vec<anyhow::Error> {
        let mut errors = Vec::new();

        // Process catchup packets
        if let Err(err) =
            M::import_catchup_packets(&mut self.obj.get_mut().catchup_state, &packet.catchup)
        {
            return vec![err];
        }

        // Process message packets
        for part in &packet.messages {
            let Some(id) = NonZeroU64::new(part.node_id).map(RpcNodeId) else {
                errors.push(anyhow::anyhow!("encountered invalid null node ID"));
                continue;
            };

            let Some(target) = self.obj.get().nodes.get(&id).cloned() else {
                errors.push(anyhow::anyhow!(
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
                errors.push(anyhow::anyhow!(
                    "attempted to send RPC to unknown path {:?} on node {:?} with id {id:?}",
                    part.path,
                    target,
                ));
                continue;
            };

            if let Err(err) = handler.call(peer, target.get().me, &part.data) {
                errors.push(err);
            }
        }

        // Clear catchup packets
        M::clear_catchup_packets(&mut self.obj.get_mut().catchup_state);

        // Report errors
        errors
    }
}

// === RpcNode === //

// Specializations
pub type RpcNodeServer = RpcNode<ServerNetMode>;
pub type RpcNodeClient = RpcNode<ClientNetMode>;

// Core
#[derive_where(Debug)]
pub struct RpcNode<M: RpcNetMode> {
    despawn: DespawnStep,

    // Dependencies
    manager: Obj<RpcManager<M>>,
    id: RpcNodeId,
    me: Entity,

    // Handlers
    message_handlers: Vec<Option<RpcMessageHandler<M::Peer>>>,
    catchup_state: M::NodeCatchupState,
}

make_extensible!(pub RpcNodeObj<M> for RpcNode where M: RpcNetMode);

impl<M: RpcNetMode> RpcNode<M> {
    pub fn new(id: RpcNodeId) -> impl CyclicCtor<Self> {
        move |me, ob| {
            let manager = me.deep_obj::<RpcManager<M>>();

            manager.get_mut().nodes.insert(id, ob.clone());

            Self {
                despawn: DespawnStep::default(),
                me,
                id,
                manager,
                message_handlers: Vec::new(),
                catchup_state: <M::NodeCatchupState>::default(),
            }
        }
    }

    pub fn id(&self) -> RpcNodeId {
        self.id
    }

    pub fn manager(&self) -> &Obj<RpcManager<M>> {
        &self.manager
    }

    pub fn entity(&self) -> Entity {
        self.me
    }

    pub fn despawn(&self) {
        self.despawn.mark();
        self.manager.get_mut().nodes.remove(&self.id);
    }
}

impl RpcNodeObj<ServerNetMode> {
    pub fn queue_catchup(&self, peer: Entity) {
        let (me, id, handlers, manager) = {
            let me = self.obj.get();
            let id = me.id;

            // Check if we have already caught up this peer.
            if me
                .manager
                .get_mut()
                .packet_queue(peer)
                .catchups
                .contains_key(&id)
            {
                return;
            }

            // Otherwise, move a bunch of state out of the node and end its borrow...
            (
                me.entity(),
                id,
                me.catchup_state.clone(),
                me.manager.clone(),
            )
        };

        // ...so that we can produce catchup packets for the node without concurrent borrows.
        let packets = handlers
            .into_iter()
            .map(|(path, gen)| (path, gen.call(peer, me)))
            .collect::<Vec<_>>();

        // Add the data to the queue.
        manager
            .get_mut()
            .packet_queue(peer)
            .catchups
            .insert(id, packets);
    }
}

// === RpcNodeBuilder === //

// Specializations
pub type RpcNodeBuilderServer<'a, P> = RpcNodeBuilder<'a, P, ServerNetMode>;
pub type RpcNodeBuilderClient<'a, P> = RpcNodeBuilder<'a, P, ClientNetMode>;

// Core
#[derive_where(Debug, Copy, Clone; P)]
pub struct RpcNodeBuilder<'a, P, M: RpcNetMode> {
    pub node: &'a Obj<RpcNode<M>>,
    pub path: P,
}

impl<'a, M: RpcNetMode> RpcNodeBuilder<'a, EmptyPathBuilder, M> {
    pub fn new(node: &'a Obj<RpcNode<M>>) -> Self {
        Self {
            node,
            path: EmptyPathBuilder,
        }
    }
}

impl<'a, P, M: RpcNetMode> RpcNodeBuilder<'a, P, M> {
    #[allow(clippy::should_implement_trait)]
    pub fn sub<C, R1, R2>(self, part: C) -> RpcNodeBuilder<'a, (P, C), M>
    where
        P: RpcPath<R1>,
        C: RpcPath<R2, Output = R1>,
    {
        RpcNodeBuilder {
            node: self.node,
            path: self.path.sub(part),
        }
    }
}

impl<P: RpcPath, M: RpcNetMode> RpcNodeBuilder<'_, P, M> {
    pub fn sender(&self) -> RpcNodeSender<M> {
        RpcNodeSender {
            node: self.node.clone(),
            path: self.path.index(),
        }
    }

    pub fn bind_message_raw(
        self,
        handler: impl 'static + Fn(M::Peer, Entity, &Bytes) -> anyhow::Result<()>,
    ) where
        P: 'static,
    {
        let mut me = self.node.get_mut();

        let slot = ensure_index(&mut me.message_handlers, self.path.index() as usize);
        debug_assert!(slot.is_none());

        *slot = Some(RpcMessageHandler::new(move |peer, target, data| {
            handler(peer, target, data)
        }));
    }

    pub fn bind_message<D>(
        self,
        handler: impl 'static + Fn(M::Peer, Entity, D) -> anyhow::Result<()>,
    ) where
        P: 'static,
        D: for<'a> Deserialize<'a>,
    {
        self.bind_message_raw(move |peer, target, data| {
            handler(peer, target, decode_packet::<D>(data)?)
        });
    }
}

impl<P: RpcPath> RpcNodeBuilderServer<'_, P> {
    pub fn bind_catchup_raw(self, handler: impl 'static + Fn(Entity, Entity) -> Bytes) {
        self.node
            .get_mut()
            .catchup_state
            .push((self.path.index(), RpcCatchupGenerator::new(handler)));
    }

    pub fn bind_catchup<D: Serialize>(self, handler: impl 'static + Fn(Entity, Entity) -> D) {
        self.bind_catchup_raw(move |peer, target| encode_packet(&handler(peer, target)));
    }
}

impl<P: RpcPath> RpcNodeBuilderClient<'_, P> {
    pub fn read_catchup_raw(self) -> anyhow::Result<Bytes> {
        let node = self.node.get();
        let manager = node.manager.get();

        manager
            .catchup_state
            .get(&(node.id, self.path.index()))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "missing catchup for {node:?} with id {:?} and path {:?}",
                    node.id,
                    self.path.make(())
                )
            })
    }

    pub fn read_catchup<D: for<'a> Deserialize<'a>>(self) -> anyhow::Result<D> {
        self.read_catchup_raw().and_then(|b| decode_packet(&b))
    }
}

#[derive(Debug, Clone)]
pub struct RpcNodeSender<M: RpcNetMode> {
    pub node: Obj<RpcNode<M>>,
    pub path: u32,
}

impl<M: RpcNetMode> RpcNodeSender<M> {
    pub fn send_raw(&self, peer: M::Peer, data: Bytes) {
        let node = self.node.get();

        node.manager
            .get_mut()
            .queue_message(peer, node.id, self.path, data);
    }

    pub fn send<D: Serialize>(&self, peer: M::Peer, data: &D) {
        self.send_raw(peer, encode_packet(data))
    }
}
