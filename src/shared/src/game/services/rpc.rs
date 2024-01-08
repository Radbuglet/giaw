use aunty::{delegate, CyclicCtor, Entity, Obj};
use extend::ext;
use rustc_hash::FxHashMap;
use std::num::NonZeroU64;

use super::transform::EntityExt;

use crate::util::lang::vec::ensure_index;

// === Path === //

// Builder
pub trait RpcPath {
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
        #[derive(Copy, Clone)]
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

// === RpcNode === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct RpcNodeId(NonZeroU64);

#[derive(Debug)]
pub struct RpcManager {
    nodes: FxHashMap<RpcNodeId, Obj<RpcNode>>,
    id_gen: NonZeroU64,
}

impl Default for RpcManager {
    fn default() -> Self {
        Self {
            nodes: Default::default(),
            id_gen: NonZeroU64::new(1).unwrap(),
        }
    }
}

impl RpcManager {
    pub fn generate_id(&mut self) -> RpcNodeId {
        self.id_gen = self.id_gen.checked_add(1).unwrap();
        RpcNodeId(self.id_gen)
    }
}

#[derive(Debug)]
pub struct RpcNode {
    manager: Obj<RpcManager>,
    id: RpcNodeId,
    rpc_handlers: Vec<Option<RpcHandler>>,
    catchup_generators: FxHashMap<u32, RpcCatchupGenerator>,
    catchup_handlers: FxHashMap<u32, RpcCatchupHandler>,
}

impl RpcNode {
    pub fn new(id: Option<RpcNodeId>) -> impl CyclicCtor<Self> {
        move |me, ob| {
            let manager = me.deep_obj::<RpcManager>();
            let id = {
                let mut manager = manager.get_mut();
                let id = id.unwrap_or_else(|| manager.generate_id());
                manager.nodes.insert(id, ob.clone());
                id
            };

            Self {
                manager,
                id,
                rpc_handlers: Vec::new(),
                catchup_generators: FxHashMap::default(),
                catchup_handlers: FxHashMap::default(),
            }
        }
    }

    pub fn bind_rpc_handler(&mut self, path: u32, handler: RpcHandler) {
        let slot = ensure_index(&mut self.rpc_handlers, path as usize);
        debug_assert!(slot.is_none());
        *slot = Some(handler);
    }

    pub fn bind_catchup_gen(&mut self, path: u32, handler: RpcCatchupGenerator) {
        self.catchup_generators.insert(path, handler);
    }

    pub fn bind_catchup_handler(&mut self, path: u32, handler: RpcCatchupHandler) {
        self.catchup_handlers.insert(path, handler);
    }

    pub fn id(&self) -> RpcNodeId {
        self.id
    }

    pub fn despawn(&mut self) {
        self.manager.get_mut().nodes.remove(&self.id);
    }
}

#[ext]
pub impl Obj<RpcNode> {
    fn builder(&self) -> RpcNodeBuilderInstance<'_, EmptyPathBuilder> {
        RpcNodeBuilderInstance {
            node: self,
            path: EmptyPathBuilder,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct RpcNodeBuilderInstance<'a, P> {
    node: &'a Obj<RpcNode>,
    path: P,
}

pub trait RpcNodeBuilder<'a, R>: Copy {
    type Path: RpcPathBuilder<R>;

    fn node(self) -> &'a Obj<RpcNode>;

    fn path(self) -> Self::Path;

    fn sub<C, R2>(self, part: C) -> RpcNodeBuilderInstance<'a, (Self::Path, C)>
    where
        C: RpcPathBuilder<R2, Output = R>;

    fn bind_rpc_handler(self, handler: impl 'static + Fn(Entity, &[u8]))
    where
        R: Default;

    fn bind_catchup_gen(
        self,
        handler: impl 'static + Fn(Entity, &mut Vec<u8>) -> anyhow::Result<()>,
    ) where
        R: Default;

    fn bind_catchup_handler(self, handler: impl 'static + Fn(&[u8]) -> anyhow::Result<()>)
    where
        R: Default;
}

impl<'a, P, R> RpcNodeBuilder<'a, R> for RpcNodeBuilderInstance<'a, P>
where
    P: RpcPathBuilder<R>,
{
    type Path = P;

    fn node(self) -> &'a Obj<RpcNode> {
        self.node
    }

    fn path(self) -> Self::Path {
        self.path
    }

    fn sub<C, R2>(self, part: C) -> RpcNodeBuilderInstance<'a, (Self::Path, C)>
    where
        C: RpcPathBuilder<R2, Output = R>,
    {
        RpcNodeBuilderInstance {
            node: self.node,
            path: self.path.sub(part),
        }
    }

    fn bind_rpc_handler(self, handler: impl 'static + Fn(Entity, &[u8]))
    where
        R: Default,
    {
        self.node
            .get_mut()
            .bind_rpc_handler(self.path.index(), RpcHandler::new(handler));
    }

    fn bind_catchup_gen(
        self,
        handler: impl 'static + Fn(Entity, &mut Vec<u8>) -> anyhow::Result<()>,
    ) where
        R: Default,
    {
        self.node
            .get_mut()
            .bind_catchup_gen(self.path.index(), RpcCatchupGenerator::new(handler));
    }

    fn bind_catchup_handler(self, handler: impl 'static + Fn(&[u8]) -> anyhow::Result<()>)
    where
        R: Default,
    {
        self.node
            .get_mut()
            .bind_catchup_handler(self.path.index(), RpcCatchupHandler::new(handler));
    }
}

delegate! {
    pub fn RpcHandler(peer: Entity, data: &[u8])
}

delegate! {
    pub fn RpcCatchupGenerator(peer: Entity, buf: &mut Vec<u8>) -> anyhow::Result<()>
}

delegate! {
    pub fn RpcCatchupHandler(data: &[u8]) -> anyhow::Result<()>
}
