use std::{fmt, num::NonZeroU64};

use bytes::Bytes;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcPacket {
    pub catchup: Vec<RpcPacketPart>,
    pub messages: Vec<RpcPacketPart>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcPacketPart {
    pub node_id: u64,
    pub path: u32,
    pub data: Bytes,
}

pub fn encode_packet(v: &impl Serialize) -> Bytes {
    Bytes::from(bincode::serialize(v).unwrap())
}

pub fn decode_packet<'a, P: Deserialize<'a>>(v: &'a Bytes) -> anyhow::Result<P> {
    bincode::deserialize(v).map_err(anyhow::Error::new)
}

// === RPC Node Common === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct RpcNodeId(pub NonZeroU64);

impl RpcNodeId {
    pub const ROOT: Self = RpcNodeId(match NonZeroU64::new(1) {
        Some(v) => v,
        None => unreachable!(),
    });
}
