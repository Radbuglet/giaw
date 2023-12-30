use std::mem;

use autoken::ImmutableBorrow;
use extend::ext;
use rustc_hash::FxHashSet;

use crate::{
    delegate,
    util::lang::{
        entity::{Entity, OwnedEntity},
        obj::Obj,
    },
};

use super::transform::Transform;

#[derive(Debug, Default)]
pub struct ActorManager {
    actors: FxHashSet<OwnedEntity>,
    queued_despawns: FxHashSet<Entity>,
}

impl ActorManager {
    pub fn spawn(&mut self) -> Entity {
        let (actor, actor_ref) = OwnedEntity::new().split_guard();
        self.actors.insert(actor);
        actor_ref
    }

    pub fn actors(&self) -> impl Iterator<Item = Entity> + '_ {
        self.actors.iter().map(OwnedEntity::entity)
    }

    pub fn queue_despawn(&mut self, actor: &Transform) {
        self.queued_despawns.insert(actor.entity());

        for descendant in actor.children() {
            self.queue_despawn(&descendant.get());
        }
    }
}

#[ext]
pub impl Obj<ActorManager> {
    fn process_despawns(&self) {
        let queued_despawns = mem::take(&mut self.get_mut().queued_despawns);

        for actor in &queued_despawns {
            if !actor.is_alive() {
                continue;
            }

            let loaner = ImmutableBorrow::new();

            if let Some(dtor) = actor.try_get::<DespawnHandler>(&loaner) {
                dtor.call();
            };
        }

        let mut me = self.get_mut();
        for actor in &queued_despawns {
            me.actors.remove(actor);
        }
    }

    fn despawn_all(&self) {
        let actors = mem::take(&mut self.get_mut().actors);
        for actor in &actors {
            let loaner = ImmutableBorrow::new();
            if let Some(dtor) = actor.try_get::<DespawnHandler>(&loaner) {
                dtor.call();
            };
        }

        drop(actors);
    }
}

delegate! {
    pub fn DespawnHandler()
}

delegate! {
    pub fn UpdateHandler()
}
