use std::{
    cell::{Cell, RefCell},
    ops::ControlFlow,
    thread::panicking,
};

use aunty::{delegate, Entity, StrongEntity};
use autoken::ImmutableBorrow;
use rustc_hash::FxHashSet;

use super::transform::Transform;

// === ActorManager === //

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

        for descendant in actor.children().iter() {
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

// === Standard Handlers === //

delegate! {
    pub fn DespawnHandler()
}

delegate! {
    pub fn UpdateHandler()
}

// === DespawnStep === //

#[derive(Debug, Clone, Default)]
pub struct DespawnStep {
    #[cfg(debug_assertions)]
    dropped: Cell<bool>,
}

impl DespawnStep {
    pub fn mark(&self) {
        #[cfg(debug_assertions)]
        {
            assert!(
                !self.dropped.get(),
                "component was despawned more than once"
            );
            self.dropped.set(true);
        }
    }
}

impl Drop for DespawnStep {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            if !panicking() {
                assert!(
                    self.dropped.get(),
                    "component was not despawned before being dropped"
                );
            }
        }
    }
}
