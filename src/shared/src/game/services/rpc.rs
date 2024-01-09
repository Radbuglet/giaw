use std::{fmt, num::NonZeroU64};

use anyhow::Context;
use aunty::{delegate, CyclicCtor, Entity, Obj};
use bytes::Bytes;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use super::transform::EntityExt;
use crate::util::lang::{drop_step::DespawnStep, vec::ensure_index};

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

// === Protocol === //

#[derive(Serialize, Deserialize)]
pub struct HelloPacket {
    pub foo: u32,
}

pub fn encode_packet(v: &impl Serialize) -> Bytes {
    Bytes::from(bincode::serialize(v).unwrap())
}

pub fn decode_packet<'a, P: Deserialize<'a>>(v: &'a Bytes) -> anyhow::Result<P> {
    bincode::deserialize(v).map_err(anyhow::Error::new)
}

// === Rpc Client === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct RpcNodeId(pub NonZeroU64);

impl RpcNodeId {
    pub const ROOT: Self = RpcNodeId(match NonZeroU64::new(1) {
        Some(v) => v,
        None => unreachable!(),
    });
}

#[derive(Debug, Default)]
pub struct RpcManager {
    errors: Vec<anyhow::Error>,
    nodes: FxHashMap<RpcNodeId, Obj<RpcNode>>,
    catchups: FxHashMap<(RpcNodeId, u32), Bytes>,
}

impl RpcManager {
    pub fn report_error(&mut self, error: anyhow::Error) {
        self.errors.push(error);
    }

    pub fn read_catchup(&self, id: RpcNodeId, path: u32) -> anyhow::Result<&Bytes> {
        self.catchups
            .get(&(id, path))
            .context("missing catchup packet")
    }
}

#[derive(Debug)]
pub struct RpcNode {
    despawn: DespawnStep,
    me: Entity,
    id: RpcNodeId,
    manager: Obj<RpcManager>,
    handlers: Vec<Option<RpcNodeHandler>>,
}

impl RpcNode {
    pub fn new(id: RpcNodeId) -> impl CyclicCtor<Self> {
        move |me, _| Self {
            despawn: DespawnStep::default(),
            me,
            id,
            manager: me.deep_obj(),
            handlers: Vec::new(),
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

    pub fn bind_handler(&mut self, path: impl RpcPathBuilder, handler: RpcNodeHandler) {
        let slot = ensure_index(&mut self.handlers, path.index() as usize);
        debug_assert!(slot.is_none());

        *slot = Some(handler);
    }

    pub fn read_catchup(&self, path: u32) -> anyhow::Result<Bytes> {
        self.manager.get().read_catchup(self.id, path).cloned()
    }

    pub fn parse_catchup<P: for<'a> Deserialize<'a>>(&self, path: u32) -> anyhow::Result<P> {
        decode_packet(&self.read_catchup(path)?)
    }

    pub fn despawn(&self) {
        self.despawn.mark();
        self.manager.get_mut().nodes.remove(&self.id);
    }
}

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
    pub fn sub<C, R1>(self, part: C) -> RpcNodeBuilder<'a, (P, C)>
    where
        P: RpcPathBuilder<R1>,
        C: RpcPathBuilder<Output = R1>,
    {
        RpcNodeBuilder {
            node: self.node,
            path: self.path.sub(part),
        }
    }
}

impl<P: RpcPathBuilder> RpcNodeBuilder<'_, P> {
    pub fn read_catchup(self) -> anyhow::Result<Bytes> {
        self.node.get().read_catchup(self.path.index())
    }

    pub fn bind_raw(self, handler: impl 'static + Fn(&Obj<RpcNode>, &Bytes) -> anyhow::Result<()>)
    where
        P: 'static,
    {
        let me = (self.node.clone(), self.path);

        self.node.get_mut().bind_handler(
            self.path,
            RpcNodeHandler::new(move |data| {
                let me = RpcNodeBuilder {
                    node: &me.0,
                    path: me.1,
                };

                me.handle_error(|_| handler(me.node, data));
            }),
        )
    }

    pub fn bind<D>(self, handler: impl 'static + Fn(&Obj<RpcNode>, D) -> anyhow::Result<()>)
    where
        P: 'static,
        D: for<'a> Deserialize<'a>,
    {
        self.bind_raw(move |me, data| handler(me, decode_packet::<D>(data)?));
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

delegate! {
    pub fn RpcNodeHandler(data: &Bytes)
}
