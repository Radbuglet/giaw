use std::{
    cell::{BorrowError, BorrowMutError, Ref, RefCell, RefMut},
    error::Error,
    fmt,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    rc::{Rc, Weak},
};

use autoken::{
    ImmutableBorrow, MutableBorrow, Nothing, PotentialImmutableBorrow, PotentialMutableBorrow,
};

use super::format::DisplayAsDebug;

// === StrongObj === //

pub struct StrongObj<T> {
    value: ManuallyDrop<Rc<RefCell<T>>>,
}

impl<T: fmt::Debug> fmt::Debug for StrongObj<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value.try_borrow() {
            Ok(value) => f.debug_tuple("StrongObj").field(&value).finish(),
            Err(err) => f.debug_tuple("StrongObj").field(&err).finish(),
        }
    }
}

impl<T: Default> Default for StrongObj<T> {
    fn default() -> Self {
        Self {
            value: Default::default(),
        }
    }
}

impl<T> From<T> for StrongObj<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T> Clone for StrongObj<T> {
    fn clone(&self) -> Self {
        Self {
            value: ManuallyDrop::new(Rc::clone(&self.value)),
        }
    }
}

impl<T> StrongObj<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: ManuallyDrop::new(Rc::new(RefCell::new(value))),
        }
    }

    pub fn new_cyclic(f: impl FnOnce(&Obj<T>) -> T) -> Self {
        Self {
            value: ManuallyDrop::new(Rc::new_cyclic(|weak| {
                RefCell::new(f(unsafe { std::mem::transmute(weak) }))
            })),
        }
    }

    pub fn downgrade(&self) -> Obj<T> {
        Obj {
            value: Rc::downgrade(&self.value),
        }
    }

    pub fn get(&self) -> CompRef<T> {
        unsafe {
            // Safety: we can't drop the cell until it is mutably borrowable
            CompRef::new_inner(ImmutableBorrow::new(), self.value.borrow())
        }
    }

    pub fn get_mut(&self) -> CompMut<T> {
        unsafe {
            // Safety: we can't drop the cell until it is mutably borrowable
            CompMut::new_inner(MutableBorrow::new(), self.value.borrow_mut())
        }
    }

    pub fn get_on_loan<'l>(&self, loaner: &'l ImmutableBorrow<T>) -> CompRef<T, Nothing<'l>> {
        unsafe {
            // Safety: we can't drop the cell until it is mutably borrowable
            CompRef::new_inner(loaner.loan(), self.value.borrow())
        }
    }

    pub fn get_mut_on_loan<'l>(&self, loaner: &'l mut MutableBorrow<T>) -> CompMut<T, Nothing<'l>> {
        unsafe {
            // Safety: we can't drop the cell until it is mutably borrowable
            CompMut::new_inner(loaner.loan(), self.value.borrow_mut())
        }
    }

    pub fn try_get<'l>(
        &self,
        loaner: &'l PotentialImmutableBorrow<T>,
    ) -> Result<CompRef<T, Nothing<'l>>, BorrowError> {
        unsafe {
            // Safety: we can't drop the cell until it is mutably borrowable
            self.value
                .try_borrow()
                .map(|guard| CompRef::new_inner(loaner.loan(), guard))
        }
    }

    pub fn try_get_mut<'l>(
        &self,
        loaner: &'l mut PotentialMutableBorrow<T>,
    ) -> Result<CompMut<T, Nothing<'l>>, BorrowMutError> {
        unsafe {
            // Safety: we can't drop the cell until it is mutably borrowable
            self.value
                .try_borrow_mut()
                .map(|guard| CompMut::new_inner(loaner.loan(), guard))
        }
    }
}

impl<T> Drop for StrongObj<T> {
    fn drop(&mut self) {
        if let Err(err) = self.value.try_borrow_mut() {
            panic!("attempted to drop StrongObj while in use: {err:?}");
        }

        // We black-box the destructor to avoid false-positives due to weird MIR flags.
        autoken::assume_black_box(|| unsafe { ManuallyDrop::drop(&mut self.value) });
    }
}

// === Obj === //

#[repr(transparent)]
pub struct Obj<T> {
    value: Weak<RefCell<T>>,
}

impl<T: fmt::Debug> fmt::Debug for Obj<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value.upgrade() {
            Some(value) => match value.try_borrow() {
                Ok(value) => f.debug_tuple("Obj").field(&value).finish(),
                Err(err) => f.debug_tuple("Obj").field(&DisplayAsDebug(&err)).finish(),
            },
            None => f
                .debug_tuple("Obj")
                .field(&DisplayAsDebug(&WeakBorrowError::Dead))
                .finish(),
        }
    }
}

impl<T> Clone for Obj<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
        }
    }
}

impl<T> Obj<T> {
    pub fn is_alive(&self) -> bool {
        self.value.strong_count() > 0
    }

    pub fn try_upgrade(&self) -> Option<StrongObj<T>> {
        self.value.upgrade().map(|obj| StrongObj {
            value: ManuallyDrop::new(obj),
        })
    }

    pub fn upgrade(&self) -> StrongObj<T> {
        self.try_upgrade()
            .expect("failed to upgrade obj: obj is dead")
    }

    unsafe fn try_upgrade_unguarded(&self) -> Option<&RefCell<T>> {
        self.is_alive().then(|| unsafe { &*self.value.as_ptr() })
    }

    unsafe fn upgrade_unguarded(&self) -> &RefCell<T> {
        assert!(self.is_alive(), "failed to get value from obj: obj is dead");
        unsafe { &*self.value.as_ptr() }
    }

    pub fn get(&self) -> CompRef<T> {
        unsafe {
            // Safety: we have at least one `StrongObj` keeping this object alive and it cannot be
            // dropped until this `CompRef` is dropped.
            CompRef::new_inner(ImmutableBorrow::new(), self.upgrade_unguarded().borrow())
        }
    }

    pub fn get_mut(&self) -> CompMut<T> {
        unsafe {
            // Safety: we have at least one `StrongObj` keeping this object alive and it cannot be
            // dropped until this `CompRef` is dropped.
            CompMut::new_inner(MutableBorrow::new(), self.upgrade_unguarded().borrow_mut())
        }
    }

    pub fn get_on_loan<'l>(&self, loaner: &'l ImmutableBorrow<T>) -> CompRef<T, Nothing<'l>> {
        unsafe {
            // Safety: we have at least one `StrongObj` keeping this object alive and it cannot be
            // dropped until this `CompRef` is dropped.
            CompRef::new_inner(loaner.loan(), self.upgrade_unguarded().borrow())
        }
    }

    pub fn get_mut_on_loan<'l>(&self, loaner: &'l mut MutableBorrow<T>) -> CompMut<T, Nothing<'l>> {
        unsafe {
            // Safety: we have at least one `StrongObj` keeping this object alive and it cannot be
            // dropped until this `CompRef` is dropped.
            CompMut::new_inner(loaner.loan(), self.upgrade_unguarded().borrow_mut())
        }
    }

    pub fn try_get<'l>(
        &self,
        loaner: &'l PotentialImmutableBorrow<T>,
    ) -> Result<CompRef<T, Nothing<'l>>, WeakBorrowError> {
        unsafe {
            // Safety: we have at least one `StrongObj` keeping this object alive and it cannot be
            // dropped until this `CompRef` is dropped.
            let Some(cell) = self.try_upgrade_unguarded() else {
                return Err(WeakBorrowError::Dead);
            };

            match cell.try_borrow() {
                Ok(guard) => Ok(CompRef::new_inner(loaner.loan(), guard)),
                Err(err) => Err(WeakBorrowError::Borrow(err)),
            }
        }
    }

    pub fn try_get_mut<'l>(
        &self,
        loaner: &'l mut PotentialMutableBorrow<T>,
    ) -> Result<CompMut<T, Nothing<'l>>, WeakBorrowMutError> {
        unsafe {
            // Safety: we have at least one `StrongObj` keeping this object alive and it cannot be
            // dropped until this `CompRef` is dropped.
            let Some(cell) = self.try_upgrade_unguarded() else {
                return Err(WeakBorrowMutError::Dead);
            };

            match cell.try_borrow_mut() {
                Ok(guard) => Ok(CompMut::new_inner(loaner.loan(), guard)),
                Err(err) => Err(WeakBorrowMutError::Borrow(err)),
            }
        }
    }
}

// === CompRef === //

pub struct CompRef<T: ?Sized, B: ?Sized = T> {
    autoken: ImmutableBorrow<B>,
    guard: Ref<'static, ()>,
    value: NonNull<T>,
}

impl<T: ?Sized, B: ?Sized> CompRef<T, B> {
    unsafe fn new_inner(autoken: ImmutableBorrow<B>, guard: Ref<'_, T>) -> Self {
        let value = NonNull::from(&*guard);
        let guard = Ref::map(guard, |_| &());
        let guard = std::mem::transmute(guard); // Erase lifetime

        Self {
            autoken,
            guard,
            value,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn clone(orig: &CompRef<T, B>) -> CompRef<T, B> {
        Self {
            autoken: orig.autoken.clone(),
            guard: Ref::clone(&orig.guard),
            value: orig.value,
        }
    }

    pub fn map<U, F>(orig: CompRef<T, B>, f: F) -> CompRef<U, B>
    where
        F: FnOnce(&T) -> &U,
        U: ?Sized,
    {
        let value = NonNull::from(f(&*orig));

        CompRef {
            autoken: orig.autoken,
            guard: orig.guard,
            value,
        }
    }

    pub fn filter_map<U, F>(orig: CompRef<T, B>, f: F) -> Result<CompRef<U, B>, CompRef<T, B>>
    where
        F: FnOnce(&T) -> Option<&U>,
        U: ?Sized,
    {
        if let Some(value) = f(&*orig) {
            let value = NonNull::from(value);

            Ok(CompRef {
                autoken: orig.autoken,
                guard: orig.guard,
                value,
            })
        } else {
            Err(orig)
        }
    }

    pub fn map_split<U, V, F>(orig: CompRef<T, B>, f: F) -> (CompRef<U, B>, CompRef<V, B>)
    where
        F: FnOnce(&T) -> (&U, &V),
        U: ?Sized,
        V: ?Sized,
    {
        let (left, right) = f(&*orig);
        let left = NonNull::from(left);
        let right = NonNull::from(right);

        (
            CompRef {
                autoken: orig.autoken.clone(),
                guard: Ref::clone(&orig.guard),
                value: left,
            },
            CompRef {
                autoken: orig.autoken,
                guard: orig.guard,
                value: right,
            },
        )
    }
}

impl<T: ?Sized, B: ?Sized> Deref for CompRef<T, B> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.value.as_ref() }
    }
}

impl<T: ?Sized + fmt::Debug, B: ?Sized> fmt::Debug for CompRef<T, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: ?Sized + fmt::Display, B: ?Sized> fmt::Display for CompRef<T, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

// === CompMut === //

pub struct CompMut<T: ?Sized, B: ?Sized = T> {
    autoken: MutableBorrow<B>,
    guard: RefMut<'static, ()>,
    value: NonNull<T>,
}

impl<T: ?Sized, B: ?Sized> CompMut<T, B> {
    unsafe fn new_inner(autoken: MutableBorrow<B>, guard: RefMut<'_, T>) -> Self {
        let value = NonNull::from(&*guard);
        let guard = RefMut::map(guard, |_| Box::leak(Box::new(())));
        let guard = std::mem::transmute(guard); // Erase lifetime

        Self {
            autoken,
            guard,
            value,
        }
    }

    pub fn map<U, F>(mut orig: CompMut<T, B>, f: F) -> CompMut<U, B>
    where
        F: FnOnce(&mut T) -> &mut U,
        U: ?Sized,
    {
        let value = NonNull::from(f(&mut *orig));

        CompMut {
            autoken: orig.autoken,
            guard: orig.guard,
            value,
        }
    }

    pub fn filter_map<U, F>(mut orig: CompMut<T, B>, f: F) -> Result<CompMut<U, B>, CompMut<T, B>>
    where
        F: FnOnce(&mut T) -> Option<&U>,
        U: ?Sized,
    {
        if let Some(value) = f(&mut *orig) {
            let value = NonNull::from(value);

            Ok(CompMut {
                autoken: orig.autoken,
                guard: orig.guard,
                value,
            })
        } else {
            Err(orig)
        }
    }

    pub fn map_split<U, V, F>(mut orig: CompMut<T, B>, f: F) -> (CompMut<U, B>, CompMut<V, B>)
    where
        F: FnOnce(&mut T) -> (&mut U, &mut V),
        U: ?Sized,
        V: ?Sized,
    {
        let (left, right) = f(&mut *orig);
        let left = NonNull::from(left);
        let right = NonNull::from(right);

        let (left_guard, right_guard) = RefMut::map_split(orig.guard, |()| {
            (Box::leak(Box::new(())), Box::leak(Box::new(())))
        });

        (
            CompMut {
                autoken: orig.autoken.assume_no_alias_clone(),
                guard: left_guard,
                value: left,
            },
            CompMut {
                autoken: orig.autoken,
                guard: right_guard,
                value: right,
            },
        )
    }
}

impl<T: ?Sized, B: ?Sized> Deref for CompMut<T, B> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.value.as_ref() }
    }
}

impl<T: ?Sized, B: ?Sized> DerefMut for CompMut<T, B> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.value.as_mut() }
    }
}

impl<T: ?Sized + fmt::Debug, B: ?Sized> fmt::Debug for CompMut<T, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: ?Sized + fmt::Display, B: ?Sized> fmt::Display for CompMut<T, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

// === WeakBorrowError === //

#[derive(Debug)]
pub enum WeakBorrowError {
    Dead,
    Borrow(BorrowError),
}

impl Error for WeakBorrowError {}

impl fmt::Display for WeakBorrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("failed to borrow obj: ")?;

        match self {
            WeakBorrowError::Dead => f.write_str("obj is dead"),
            WeakBorrowError::Borrow(err) => fmt::Display::fmt(err, f),
        }
    }
}

#[derive(Debug)]
pub enum WeakBorrowMutError {
    Dead,
    Borrow(BorrowMutError),
}

impl Error for WeakBorrowMutError {}

impl fmt::Display for WeakBorrowMutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("failed to borrow obj: ")?;

        match self {
            WeakBorrowMutError::Dead => f.write_str("obj is dead"),
            WeakBorrowMutError::Borrow(err) => fmt::Display::fmt(err, f),
        }
    }
}
