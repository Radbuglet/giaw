// === Path === //

// Builder
pub trait RpcPath {
    const MAX: u64;

    fn as_index(&self) -> u64;
}

impl RpcPath for () {
    const MAX: u64 = 1;

    fn as_index(&self) -> u64 {
        0
    }
}

pub trait RpcPathBuilder<R>: Sized + Copy {
    type Output: RpcPath;

    fn index(self) -> u64
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

// Macro
#[doc(hidden)]
pub mod rpc_path_macro_internals {
    pub use {
        super::RpcPath,
        std::{primitive::u64, unreachable},
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
			const MAX: $crate::game::services::rpc::rpc_path_macro_internals::u64 = 0
				$(+ <($($variant_ty)?) as $crate::game::services::rpc::rpc_path_macro_internals::RpcPath>::MAX)*;

			fn as_index(&self) -> $crate::game::services::rpc::rpc_path_macro_internals::u64 {
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

// === Tests === //

#[cfg(test)]
mod tests {
    use super::*;

    rpc_path! {
        pub enum GameRootRpcs {
            SendMessage(MessageRpcs),
        }

        pub enum MessageRpcs {
            SendSystem,
            SendPlayer,
            SettingsNetVal(ReplicatedValueRpcs),
        }

        pub enum ReplicatedValueRpcs {
            SetValue,
            ChangeValue,
        }
    }

    #[test]
    fn whee() {
        woo(GameRootRpcs::SendMessage);
    }

    fn woo(v: impl RpcPathBuilder<MessageRpcs>) {
        assert_eq!(v.sub(MessageRpcs::SendSystem).index(), 0);
        assert_eq!(v.sub(MessageRpcs::SendPlayer).index(), 1);
        waz(v.sub(MessageRpcs::SettingsNetVal));
    }

    fn waz(v: impl RpcPathBuilder<ReplicatedValueRpcs>) {
        assert_eq!(v.sub(ReplicatedValueRpcs::SetValue).index(), 2);
        assert_eq!(v.sub(ReplicatedValueRpcs::ChangeValue).index(), 3);
    }
}
