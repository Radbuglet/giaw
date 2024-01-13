use std::num::NonZeroU64;

use anyhow::Context;
use aunty::{delegate, make_extensible, CyclicCtor, Entity, Obj};
use bytes::Bytes;
use giaw_shared::{
    game::services::{
        actors::DespawnStep,
        rpc::{decode_packet, EmptyPathBuilder, RpcNodeId, RpcPacket, RpcPathBuilder},
        transform::EntityExt,
    },
    util::lang::vec::ensure_index,
};
use rustc_hash::FxHashMap;
use serde::Deserialize;

// === RpcManager === //

delegate! {
    pub fn RpcMessageHandler(data: &Bytes)
}

#[derive(Debug, Default)]
pub struct RpcManager {
    errors: Vec<anyhow::Error>,
    nodes: FxHashMap<RpcNodeId, Obj<RpcNode>>,
    catchups: FxHashMap<(RpcNodeId, u32), Bytes>,
}

make_extensible!(pub RpcManagerObj for RpcManager);

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

impl RpcManagerObj {
    #[must_use]
    pub fn process_packet(&self, packet: &RpcPacket) -> Vec<anyhow::Error> {
        // Push catchup packets
        {
            let mut me = self.obj.get_mut();

            for part in &packet.catchup {
                let Some(id) = NonZeroU64::new(part.node_id).map(RpcNodeId) else {
                    me.report_error(anyhow::anyhow!("encountered invalid null node ID"));
                    continue;
                };

                me.catchups.insert((id, part.path), part.data.clone());
            }
        }

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
                .handlers
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

            handler.call(&part.data);
        }

        // Clear catchup and report errors
        let mut me = self.obj.get_mut();
        me.catchups.clear();
        std::mem::take(&mut me.errors)
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

#[derive(Debug)]
pub struct RpcNode {
    despawn: DespawnStep,
    me: Entity,
    id: RpcNodeId,
    manager: Obj<RpcManager>,
    handlers: Vec<Option<RpcMessageHandler>>,
}

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
                handlers: Vec::new(),
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

    pub fn bind_handler(&mut self, path: impl RpcPathBuilder, handler: RpcMessageHandler) {
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
            RpcMessageHandler::new(move |data| {
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
