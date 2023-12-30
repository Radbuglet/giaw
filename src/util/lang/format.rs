use std::fmt;

pub struct DisplayAsDebug<'a, T: ?Sized>(pub &'a T);

impl<T: ?Sized + fmt::Display> fmt::Debug for DisplayAsDebug<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
