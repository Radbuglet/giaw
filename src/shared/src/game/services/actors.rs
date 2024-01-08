use std::{cell::RefCell, ops::ControlFlow};

use aunty::{delegate, Entity, StrongEntity};
use autoken::ImmutableBorrow;
use rustc_hash::FxHashSet;

use super::transform::Transform;

#[derive(Debug, Default)]
pub struct ActorManager {
    actors: RefCell<FxHashSet<StrongEntity>>,
    queued_spawns: RefCell<Vec<StrongEntity>>,
    queued_despawns: RefCell<FxHashSet<Entity>>,
}

impl ActorManager {
    pub fn spawn(&self) -> Entity {
        let (actor, actor_ref) = StrongEntity::new().split_guard();

        if let Ok(mut actors) = self.actors.try_borrow_mut() {
            actors.insert(actor);
        } else {
            self.queued_spawns.borrow_mut().push(actor);
        }

        actor_ref
    }

    pub fn iter_actors<B>(&self, mut f: impl FnMut(Entity) -> ControlFlow<B>) -> ControlFlow<B> {
        let actors = self.actors.borrow();
        for actor in &*actors {
            f(actor.entity())?;
        }
        drop(actors);

        if let Ok(mut actors) = self.actors.try_borrow_mut() {
            actors.extend(self.queued_spawns.borrow_mut().drain(..));
        }

        ControlFlow::Continue(())
    }

    pub fn queue_despawn(&self, actor: &Transform) {
        self.queued_despawns.borrow_mut().insert(actor.entity());

        for descendant in actor.children() {
            self.queue_despawn(&descendant.get());
        }
    }

    pub fn process_despawns(&self) {
        let queued_despawns = self.queued_despawns.take();

        for actor in &queued_despawns {
            if !actor.is_alive() {
                continue;
            }

            let loaner = ImmutableBorrow::new();

            if let Some(dtor) = actor.try_get::<DespawnHandler>(&loaner) {
                dtor.call();
            };
        }

        let mut actors = self.actors.borrow_mut();

        for actor in &queued_despawns {
            actors.remove(actor);
        }
    }

    pub fn despawn_all(&self) {
        let actors = self.actors.take();

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
