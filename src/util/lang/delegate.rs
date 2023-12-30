use std::{
    fmt,
    ops::{Deref, DerefMut},
};

// === Injectors === //

pub trait FuncMethodInjectorRef<T: ?Sized> {
    type Guard<'a>: Deref<Target = T>;
    type Injector;

    const INJECTOR: Self::Injector;
}

pub trait FuncMethodInjectorMut<T: ?Sized> {
    type Guard<'a>: DerefMut<Target = T>;
    type Injector;

    const INJECTOR: Self::Injector;
}

// === Delegate Traits === //

pub trait Delegate: fmt::Debug + Clone {}

// === Delegate === //

#[doc(hidden)]
pub mod delegate_macro_internal {
    use std::{mem::MaybeUninit, ops::DerefMut};

    // === Re-exports === //

    pub use {
        super::{Delegate, FuncMethodInjectorMut, FuncMethodInjectorRef},
        std::{
            clone::Clone,
            convert::From,
            fmt,
            marker::PhantomData,
            ops::{Deref, Fn},
            panic::Location,
            rc::Rc,
            stringify,
        },
    };

    // === Private helpers === //

    pub trait FuncMethodInjectorRefGetGuard<T: ?Sized> {
        type GuardHelper<'a>: Deref<Target = T>;
    }

    impl<G, T> FuncMethodInjectorRefGetGuard<T> for G
    where
        T: ?Sized,
        G: FuncMethodInjectorRef<T>,
    {
        type GuardHelper<'a> = G::Guard<'a>;
    }

    pub trait FuncMethodInjectorMutGetGuard<T: ?Sized> {
        type GuardHelper<'a>: DerefMut<Target = T>;
    }

    impl<G, T> FuncMethodInjectorMutGetGuard<T> for G
    where
        T: ?Sized,
        G: FuncMethodInjectorMut<T>,
    {
        type GuardHelper<'a> = G::Guard<'a>;
    }

    // N.B. this function is not marked as unsafe because `#[forbid(dead_code)]` may be used in
    // userland crates.
    #[allow(unsafe_code)] // TODO: Move to `core`
    pub fn uber_dangerous_transmute_this_is_unsound<A, B>(a: A) -> B {
        unsafe {
            let mut a = MaybeUninit::<A>::new(a);
            a.as_mut_ptr().cast::<B>().read()
        }
    }
}

#[macro_export]
macro_rules! delegate {
    // === With injector === //
    (
        $(#[$attr_meta:meta])*
        $vis:vis fn $name:ident
            $(
                <$($generic:ident),* $(,)?>
                $(<$($fn_lt:lifetime),* $(,)?>)?
            )?
            (
                &$inj_lt:lifetime self [$($inj_name:ident: $inj:ty),* $(,)?]
                $(, $para_name:ident: $para:ty)* $(,)?
            ) $(-> $ret:ty)?
        $(as deriving $deriving:path $({ $($deriving_args:tt)* })? )*
        $(where $($where_token:tt)*)?
    ) => {
        $crate::util::lang::delegate::delegate! {
            $(#[$attr_meta])*
            $vis fn $name
                < $($($generic),*)? >
                < $inj_lt, $($($($fn_lt),*)?)? >
                (
                    $($inj_name: $inj,)*
                    $($para_name: $para,)*
                ) $(-> $ret)?
            $(as deriving $deriving $({ $($deriving_args)* })? )*
            $(where $($where_token)*)?
        }

        impl$(<$($generic),*>)? $name $(<$($generic),*>)?
        $(where
            $($where_token)*
        )? {
            #[allow(unused)]
            #[cfg_attr(debug_assertions, track_caller)]
            pub fn new_method_ref<Injector, Receiver, Func>(_injector: Injector, handler: Func) -> Self
            where
                Injector: 'static + $crate::util::lang::delegate::delegate_macro_internal::FuncMethodInjectorRefGetGuard<Receiver>,
                Injector: $crate::util::lang::delegate::delegate_macro_internal::FuncMethodInjectorRef<
                    Receiver,
                    Injector = for<
                        $inj_lt
                        $($(
                            $(,$fn_lt)*
                        )?)?
                    > fn(
                        &$inj_lt (),
                        $(&mut $inj),*
                    ) -> Injector::GuardHelper<$inj_lt>>,
                Receiver: ?Sized + 'static,
                Func: 'static
                    + for<$inj_lt $($( $(,$fn_lt)* )?)?> Fn(
                        &Receiver,
                        $($inj,)*
                        $($para,)*
                    ) $(-> $ret)?,
            {
                Self::new(move |$(mut $inj_name,)* $($para_name,)*| {
                    let guard = Injector::INJECTOR(&(), $(&mut $inj_name,)*);

                    handler(&*guard, $($inj_name,)* $($para_name,)*)
                })
            }

            #[allow(unused)]
            #[cfg_attr(debug_assertions, track_caller)]
            pub fn new_method_mut<Injector, Receiver, Func>(_injector: Injector, handler: Func) -> Self
            where
                Injector: 'static + $crate::util::lang::delegate::delegate_macro_internal::FuncMethodInjectorMutGetGuard<Receiver>,
                Injector: $crate::util::lang::delegate::delegate_macro_internal::FuncMethodInjectorMut<
                    Receiver,
                    Injector = for<
                        $inj_lt
                        $($(
                            $(,$fn_lt)*
                        )?)?
                    > fn(
                        &$inj_lt (),
                        $(&mut $inj),*
                    ) -> Injector::GuardHelper<$inj_lt>>,
                Receiver: ?Sized + 'static,
                Func: 'static
                    + for<$inj_lt $($( $(,$fn_lt)* )?)?> Fn(
                        &mut Receiver,
                        $($inj,)*
                        $($para,)*
                    ) $(-> $ret)?,
            {
                Self::new(move |$(mut $inj_name,)* $($para_name,)*| {
                    let mut guard = Injector::INJECTOR(&(), $(&mut $inj_name,)*);

                    handler(&mut *guard, $($inj_name,)* $($para_name,)*)
                })
            }
        }
    };

    // === Without injector === //
    (
        $(#[$attr_meta:meta])*
        $vis:vis fn $name:ident
            $(
                <$($generic:ident),* $(,)?>
                $(<$($fn_lt:lifetime),* $(,)?>)?
            )?
            ($($para_name:ident: $para:ty),* $(,)?) $(-> $ret:ty)?
        $(as deriving $deriving:path $({ $($deriving_args:tt)* })? )*
        $(where $($where_token:tt)*)?
    ) => {
        $(#[$attr_meta])*
        $vis struct $name <
            $($($generic,)*)?
            Marker = (),
            Handler: ?Sized =
                $($(for<$($fn_lt),*>)?)?
                dyn $crate::util::lang::delegate::delegate_macro_internal::Fn(
                    $crate::util::lang::delegate::delegate_macro_internal::PhantomData<$name<$($($generic,)*)? Marker, ()>>
                    $(,$para)*
                ) $(-> $ret)?,
        >
        $(where
            $($where_token)*
        )? {
            _ty: (
                $crate::util::lang::delegate::delegate_macro_internal::PhantomData<fn() -> Marker>,
                $($($crate::util::lang::delegate::delegate_macro_internal::PhantomData<fn() -> $generic>,)*)?
            ),
            #[cfg(debug_assertions)]
            defined: &'static $crate::util::lang::delegate::delegate_macro_internal::Location<'static>,
            handler: $crate::util::lang::delegate::delegate_macro_internal::Rc<Handler>,
        }

        #[allow(unused)]
        impl<$($($generic),*)?> $name<$($($generic,)*)?>
        $(where
            $($where_token)*
        )? {
            #[cfg_attr(debug_assertions, track_caller)]
            pub fn new<Func>(handler: Func) -> Self
            where
                Func: 'static +
                    $($(for<$($fn_lt),*>)?)?
                        Fn($($para),*) $(-> $ret)?,
            {
                Self::new_raw($crate::util::lang::delegate::delegate_macro_internal::Rc::new(
                    move |_marker $(,$para_name)*| handler($($para_name),*)
                ))
            }
        }

        #[allow(unused)]
        impl<
            $($($generic,)*)?
            Marker,
            Handler: ?Sized +
                $($(for<$($fn_lt),*>)?)?
                $crate::util::lang::delegate::delegate_macro_internal::Fn(
                    $crate::util::lang::delegate::delegate_macro_internal::PhantomData<$name<$($($generic,)*)? Marker, ()>>
                    $(,$para)*
                ) $(-> $ret)?,
        > $name <$($($generic,)*)? Marker, Handler>
        $(where
            $($where_token)*
        )? {
            #[cfg_attr(debug_assertions, track_caller)]
            pub fn new_raw(handler: $crate::util::lang::delegate::delegate_macro_internal::Rc<Handler>) -> Self {
                Self {
                    _ty: (
                        $crate::util::lang::delegate::delegate_macro_internal::PhantomData::<fn() -> Marker>,
                        $($($crate::util::lang::delegate::delegate_macro_internal::PhantomData::<fn() -> $generic>,)*)?
                    ),
                    #[cfg(debug_assertions)]
                    defined: $crate::util::lang::delegate::delegate_macro_internal::Location::caller(),
                    handler,
                }
            }

            #[allow(non_camel_case_types)]
            pub fn call<$($($($fn_lt,)*)?)? $($para_name,)* __Out>(&self $(,$para_name: $para_name)*) -> __Out
            where
                $($(for<$($fn_lt,)*>)?)? fn($($para,)*) $(-> $ret)?: $crate::util::lang::delegate::delegate_macro_internal::Fn($($para_name,)*) -> __Out,
            {
                $crate::util::lang::delegate::delegate_macro_internal::uber_dangerous_transmute_this_is_unsound(
                    (self.handler)(
                        $crate::util::lang::delegate::delegate_macro_internal::PhantomData,
                        $($crate::util::lang::delegate::delegate_macro_internal::uber_dangerous_transmute_this_is_unsound($para_name),)*
                    )
                )
            }
        }

        impl<
            Func: 'static +
                $($(for<$($fn_lt),*>)?)?
                    Fn($($para),*) $(-> $ret)?
            $(, $($generic),*)?
        > $crate::util::lang::delegate::delegate_macro_internal::From<Func> for $name $(<$($generic),*>)?
        $(where
            $($where_token)*
        )? {
            #[cfg_attr(debug_assertions, track_caller)]
            fn from(handler: Func) -> Self {
                Self::new(handler)
            }
        }

        impl<$($($generic,)*)? Marker, Handler: ?Sized> $crate::util::lang::delegate::delegate_macro_internal::fmt::Debug for $name<$($($generic,)*)? Marker, Handler>
        $(where
            $($where_token)*
        )? {
            fn fmt(&self, fmt: &mut $crate::util::lang::delegate::delegate_macro_internal::fmt::Formatter) -> $crate::util::lang::delegate::delegate_macro_internal::fmt::Result {
                fmt.write_str("delegate::")?;
                fmt.write_str($crate::util::lang::delegate::delegate_macro_internal::stringify!($name))?;
                fmt.write_str("(")?;
                $(
                    fmt.write_str($crate::util::lang::delegate::delegate_macro_internal::stringify!($para))?;
                )*
                fmt.write_str(")")?;

                #[cfg(debug_assertions)]
                {
                    fmt.write_str(" @ ")?;
                    fmt.write_str(self.defined.file())?;
                    fmt.write_str(":")?;
                    $crate::util::lang::delegate::delegate_macro_internal::fmt::Debug::fmt(&self.defined.line(), fmt)?;
                    fmt.write_str(":")?;
                    $crate::util::lang::delegate::delegate_macro_internal::fmt::Debug::fmt(&self.defined.column(), fmt)?;
                }

                Ok(())
            }
        }

        impl<$($($generic,)*)? Marker, Handler: ?Sized> $crate::util::lang::delegate::delegate_macro_internal::Clone for $name<$($($generic,)*)? Marker, Handler>
        $(where
            $($where_token)*
        )? {
            fn clone(&self) -> Self {
                Self {
                    _ty: (
                        $crate::util::lang::delegate::delegate_macro_internal::PhantomData::<fn() -> Marker>,
                        $($($crate::util::lang::delegate::delegate_macro_internal::PhantomData::<fn() -> $generic>,)*)?
                    ),
                    #[cfg(debug_assertions)]
                    defined: self.defined,
                    handler: $crate::util::lang::delegate::delegate_macro_internal::Clone::clone(&self.handler),
                }
            }
        }

        impl<$($($generic,)*)? Marker, Handler: ?Sized> $crate::util::lang::delegate::delegate_macro_internal::Delegate for $name<$($($generic,)*)? Marker, Handler>
        $(where
            $($where_token)*
        )?
        {
        }

        $crate::util::lang::delegate::delegate! {
            @__internal_forward_derives

            $(#[$attr_meta])*
            $vis fn $name
                $(
                    <$($generic,)*>
                    $(<$($fn_lt,)*>)?
                )?
                ($($para_name: $para,)*) $(-> $ret)?
            $(as deriving $deriving $({ $($deriving_args)* })? )*
            $(where $($where_token)*)?
        }
    };

    // === Helpers === //
    (
        @__internal_forward_derives

        $(#[$attr_meta:meta])*
        $vis:vis fn $name:ident
            $(
                <$($generic:ident),* $(,)?>
                $(<$($fn_lt:lifetime),* $(,)?>)?
            )?
            ($($para_name:ident: $para:ty),* $(,)?) $(-> $ret:ty)?
        as deriving $first_deriving:path $({ $($first_deriving_args:tt)* })?
        $(as deriving $next_deriving:path $({ $($next_deriving_args:tt)* })? )*
        $(where $($where_token:tt)*)?
    ) => {
        $first_deriving! {
            args { $($($first_deriving_args)*)? }

            $(#[$attr_meta])*
            $vis fn $name
                $(
                    <$($generic,)*>
                    $(<$($fn_lt,)*>)?
                )?
                ($($para_name: $para,)*) $(-> $ret)?
            $(where $($where_token)*)?
        }

        $crate::util::lang::delegate::delegate! {
            @__internal_forward_derives

            $(#[$attr_meta])*
            $vis fn $name
                $(
                    <$($generic,)*>
                    $(<$($fn_lt,)*>)?
                )?
                ($($para_name: $para,)*) $(-> $ret)?
            $(as deriving $next_deriving $({ $($next_deriving_args)* })?)*
            $(where $($where_token)*)?
        }
    };
    (
        @__internal_forward_derives

        $(#[$attr_meta:meta])*
        $vis:vis fn $name:ident
            $(
                <$($generic:ident),* $(,)?>
                $(<$($fn_lt:lifetime),* $(,)?>)?
            )?
            ($($para_name:ident: $para:ty),* $(,)?) $(-> $ret:ty)?
        $(where $($where_token:tt)*)?
    ) => { /* base case */};

    (@__internal_or_unit $ty:ty) => { $ty };
    (@__internal_or_unit) => { () };
}

pub use delegate;
