use std::{fmt, hash, marker::PhantomData, num::NonZeroU64};

use aunty::{delegate, make_extensible, CyclicCtor, Entity, Obj};
use bytes::Bytes;
use derive_where::derive_where;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

// === Path === //

// Builder
pub trait RpcPath: fmt::Debug {
    const MAX: u32;

    fn as_index(&self) -> u32;
}

impl RpcPath for () {
    const MAX: u32 = 1;

    fn as_index(&self) -> u32 {
        0
    }
}

pub trait RpcPathBuilder<R = ()>: Copy {
    type Output: RpcPath;

    fn index(self) -> u32
    where
        R: Default,
    {
        self.make(R::default()).as_index()
    }

    fn make(self, remainder: R) -> Self::Output;

    fn sub<C, R2>(self, part: C) -> (Self, C)
    where
        C: RpcPathBuilder<R2, Output = R>,
    {
        (self, part)
    }
}

impl<F, R, O> RpcPathBuilder<R> for F
where
    O: RpcPath,
    F: Copy + FnOnce(R) -> O,
{
    type Output = O;

    fn make(self, value: R) -> Self::Output {
        self(value)
    }
}

impl<R, A, B> RpcPathBuilder<R> for (A, B)
where
    A: RpcPathBuilder<B::Output>,
    B: RpcPathBuilder<R>,
{
    type Output = A::Output;

    fn make(self, remainder: R) -> Self::Output {
        self.0.make(self.1.make(remainder))
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct EmptyPathBuilder;

impl<R: RpcPath> RpcPathBuilder<R> for EmptyPathBuilder {
    type Output = R;

    fn make(self, remainder: R) -> Self::Output {
        remainder
    }
}

// Macro
#[doc(hidden)]
pub mod rpc_path_macro_internals {
    pub use {
        super::RpcPath,
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
        impl $crate::game::services::rpc::rpc_path_macro_internals::RpcPath for $enum_name {
            const MAX: $crate::game::services::rpc::rpc_path_macro_internals::u32 = 0
                $(+ <($($variant_ty)?) as $crate::game::services::rpc::rpc_path_macro_internals::RpcPath>::MAX)*;

            fn as_index(&self) -> $crate::game::services::rpc::rpc_path_macro_internals::u32 {
                let offset = 0;

                $(
                    if let Self::$variant_name(var) = self {
                        return offset + $crate::game::services::rpc::rpc_path_macro_internals::RpcPath::as_index(var);
                    }

                    let offset = offset + <($($variant_ty)?) as $crate::game::services::rpc::rpc_path_macro_internals::RpcPath>::MAX;
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
}

impl RpcNetMode for ServerNetMode {
    type Peer = Entity;
}

impl RpcNetMode for ClientNetMode {
    type Peer = ();
}

// Core

delegate! {
    pub fn RpcMessageHandler<P>(peer: P, node: Entity, data: &Bytes) -> anyhow::Result<()>
}

#[derive_where(Debug, Default)]
pub struct RpcManager<M: RpcNetMode> {
    _ty: PhantomData<M>,
    nodes: FxHashMap<RpcNodeId, Obj<RpcNode<M>>>,
    message_queues: FxHashMap<M::Peer, Vec<RpcPacketMessage>>,
}

make_extensible!(pub RpcManagerObj<M> for RpcManager where M: RpcNetMode);

impl<M: RpcNetMode> RpcManager<M> {
    pub fn queue_message(&mut self, peer: M::Peer, node: RpcNodeId, path: u32, data: Bytes) {
        self.message_queues
            .entry(peer)
            .or_default()
            .push(RpcPacketMessage {
                node_id: node.0.get(),
                path,
                data,
            });
    }

    pub fn drain_queues(&mut self) -> impl Iterator<Item = (M::Peer, Vec<RpcPacketMessage>)> + '_ {
        self.message_queues.drain()
    }
}

impl<M: RpcNetMode> RpcManagerObj<M> {
    #[must_use]
    pub fn process_packet(&self, peer: M::Peer, packet: &RpcPacket) -> Vec<anyhow::Error> {
        let mut errors = Vec::new();

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
                .handlers
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

        // Reject catchup packets
        if !packet.catchup.is_empty() {
            errors.push(anyhow::anyhow!("client attempted to send catchup packet"));
        }

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
    handlers: Vec<Option<RpcMessageHandler<M::Peer>>>,
}

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
                handlers: Vec::new(),
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

// === RpcNodeBuilder === //

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
        P: RpcPathBuilder<R1>,
        C: RpcPathBuilder<R2, Output = R1>,
    {
        RpcNodeBuilder {
            node: self.node,
            path: self.path.sub(part),
        }
    }
}

impl<P: RpcPathBuilder, M: RpcNetMode> RpcNodeBuilder<'_, P, M> {
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

        let slot = ensure_index(&mut me.handlers, self.path.index() as usize);
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
