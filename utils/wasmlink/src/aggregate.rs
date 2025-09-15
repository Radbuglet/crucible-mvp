use std::{fmt, marker::PhantomData};

use bytemuck::TransparentWrapper;
use derive_where::derive_where;

use crate::{
    FfiPtr, GuestInvokeContext, GuestMemoryContext, Marshal, Strategy, ffi_offset,
    utils::impl_tuples,
};

// === Tuple Marshalling === //

// FfiTuple
#[derive(TransparentWrapper)]
#[derive_where(Copy, Clone; S::ReprC)]
#[repr(transparent)]
pub struct FfiTuple<S: TupleShape> {
    pub raw: S::ReprC,
}

impl<S: TupleShape> fmt::Debug for FfiTuple<S>
where
    for<'a> S::AsRef<'a>: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.decode_ref().fmt(f)
    }
}

impl<S: TupleShape> FfiTuple<S> {
    pub fn new(value: S) -> Self {
        Self {
            raw: S::encode(value),
        }
    }

    pub fn decode(self) -> S {
        S::decode(self.raw)
    }

    pub fn decode_ref(&self) -> S::AsRef<'_> {
        S::decode_ref(&self.raw)
    }

    pub fn decode_mut(&mut self) -> S::AsMut<'_> {
        S::decode_mut(&mut self.raw)
    }
}

impl<S: TupleShape> From<S> for FfiTuple<S> {
    fn from(value: S) -> Self {
        Self::new(value)
    }
}

pub trait TupleShape: Sized {
    type ReprC;

    type AsRef<'a>
    where
        Self: 'a;

    type AsMut<'a>
    where
        Self: 'a;

    fn encode(me: Self) -> Self::ReprC;

    fn decode(me: Self::ReprC) -> Self;

    fn decode_ref(me: &Self::ReprC) -> Self::AsRef<'_>;

    fn decode_mut(me: &mut Self::ReprC) -> Self::AsMut<'_>;
}

macro_rules! impl_ffi_tuple_shape {
    ($($para:ident:$field:tt),*) => {
        const _: () = {
            #[derive(Copy, Clone)]
            #[repr(C)]
            pub struct ReprC<$($para,)*>($(pub $para,)*);

            #[allow(unused, clippy::unused_unit)]
            impl<$($para,)*> TupleShape for ($($para,)*) {
                type ReprC = ReprC<$($para,)*>;
                type AsRef<'a> = ($(&'a $para,)*) where Self: 'a;
                type AsMut<'a> = ($(&'a mut $para,)*) where Self: 'a;

                fn encode(me: Self) -> Self::ReprC {
                    ReprC($(me.$field),*)
                }

                fn decode(me: Self::ReprC) -> Self {
                    ($(me.$field,)*)
                }

                fn decode_ref(me: &Self::ReprC) -> Self::AsRef<'_> {
                    ($(&me.$field,)*)
                }

                fn decode_mut(me: &mut Self::ReprC) -> Self::AsMut<'_> {
                    ($(&mut me.$field,)*)
                }
            }
        };
    };
}

impl_tuples!(impl_ffi_tuple_shape);

// Marshal implementations
const _: () = {
    trait Helper {
        type HostboundInner<'a>;
        type GuestboundInner;
    }

    macro_rules! impl_tuple_marshal {
        ($($para:ident:$field:tt),*) => {
            impl<$($para: Marshal,)*> Marshal for ($($para,)*) {
                type Strategy = ($($para::Strategy,)*);
            }

            impl<$($para: Strategy,)*> Helper for ($($para,)*) {
                type HostboundInner<'a> = ($($para::Hostbound<'a>,)*);
                type GuestboundInner = ($($para::Guestbound,)*);
            }

            impl<$($para: Strategy,)*> Strategy for ($($para,)*) {
                type Hostbound<'a> = FfiTuple<($($para::Hostbound<'a>,)*)>;
                type HostboundView = ($($para::HostboundView,)*);
                type Guestbound = FfiTuple<($($para::Guestbound,)*)>;
                type GuestboundView<'a> = ($($para::GuestboundView<'a>,)*);

                fn decode_hostbound(
                    cx: &(impl ?Sized + GuestMemoryContext),
                    ptr: FfiPtr<Self::Hostbound<'static>>,
                ) -> anyhow::Result<Self::HostboundView> {
                    Ok(($(
                        <<$para as Marshal>::Strategy>::decode_hostbound(
                            cx,
                            ptr.cast().field(ffi_offset!(
                                <Self as Helper>::HostboundInner<'_>, $field,
                            )),
                        )?,
                    )*))
                }

                fn encode_guestbound(
                    cx: &mut impl GuestInvokeContext,
                    out_ptr: FfiPtr<Self::Guestbound>,
                    value: &Self::GuestboundView<'_>,
                ) -> anyhow::Result<()> {
                    $(
                        <<$para as Marshal>::Strategy>::encode_guestbound(
                            cx,
                            out_ptr.cast().field(ffi_offset!(
                                <Self as Helper>::GuestboundInner, $field,
                            )),
                            &value.$field,
                        )?;
                    )*

                    Ok(())
                }
            }
        };
    }

    impl_tuples!(impl_tuple_marshal; no_unit);
};

// === Struct Marshalling === //

// VariantSelector
pub trait VariantSelector: Sized {
    type Output<T: Strategy>;
}

#[non_exhaustive]
pub struct MarkerVariant;

const _: () = {
    pub enum Never {}

    impl VariantSelector for MarkerVariant {
        type Output<T: Strategy> = Never;
    }
};

pub struct HostboundVariant<'a> {
    _ty: PhantomData<&'a ()>,
}

impl<'a> VariantSelector for HostboundVariant<'a> {
    type Output<T: Strategy> = T::Hostbound<'a>;
}

#[non_exhaustive]
pub struct HostboundViewVariant;

impl VariantSelector for HostboundViewVariant {
    type Output<T: Strategy> = T::HostboundView;
}

#[non_exhaustive]
pub struct GuestboundVariant;

impl VariantSelector for GuestboundVariant {
    type Output<T: Strategy> = T::Guestbound;
}

pub struct GuestboundViewVariant<'a> {
    _ty: PhantomData<&'a ()>,
}

impl<'a> VariantSelector for GuestboundViewVariant<'a> {
    type Output<T: Strategy> = T::GuestboundView<'a>;
}

// Macro
pub mod marshal_struct_internals {
    pub use {
        crate::{
            FfiPtr, GuestInvokeContext, GuestMemoryContext, GuestboundVariant,
            GuestboundViewVariant, HostboundVariant, HostboundViewVariant, MarkerVariant, Marshal,
            Strategy, VariantSelector, ffi_offset,
        },
        anyhow::Result,
    };
}

#[macro_export]
macro_rules! marshal_struct {
    ($(
        $(#[$($item_meta:tt)*])*
        $item_vis:vis struct $item_name:ident {
            $(
                $(#[$($field_meta:tt)*])*
                $field_vis:vis $field_name:ident: $field_ty:ty
            ),+
            $(,)?
        }
    )*) => {$(
        $(#[$($item_meta:tt)*])*
        #[repr(C)]
        $item_vis struct $item_name<
            V: $crate::marshal_struct_internals::VariantSelector = $crate::marshal_struct_internals::MarkerVariant
        > {$(
            $(#[$($field_meta)*])*
            $field_vis $field_name: <V as $crate::marshal_struct_internals::VariantSelector>::Output<
                <$field_ty as $crate::marshal_struct_internals::Marshal>::Strategy
            >,
        )*}

        impl $crate::marshal_struct_internals::Marshal for $item_name {
            type Strategy = Self;
        }

        impl $crate::marshal_struct_internals::Strategy for $item_name {
            type Hostbound<'a> = $item_name<$crate::marshal_struct_internals::HostboundVariant<'a>>;
            type HostboundView = $item_name<$crate::marshal_struct_internals::HostboundViewVariant>;
            type Guestbound = $item_name<$crate::marshal_struct_internals::GuestboundVariant>;
            type GuestboundView<'a> = $item_name<$crate::marshal_struct_internals::GuestboundViewVariant<'a>>;

            fn decode_hostbound(
                cx: &(impl ?Sized + $crate::marshal_struct_internals::GuestMemoryContext),
                ptr: $crate::marshal_struct_internals::FfiPtr<Self::Hostbound<'static>>,
            ) -> $crate::marshal_struct_internals::Result<Self::HostboundView> {
                Ok(Self::HostboundView {$(
                    $field_name: <<$field_ty as $crate::marshal_struct_internals::Marshal>::Strategy>::decode_hostbound(
                        cx,
                        ptr.field($crate::marshal_struct_internals::ffi_offset!(
                            Self::Hostbound<'static>, $field_name,
                        )),
                    )?,
                )*})
            }

            fn encode_guestbound(
                cx: &mut impl $crate::marshal_struct_internals::GuestInvokeContext,
                out_ptr: $crate::marshal_struct_internals::FfiPtr<Self::Guestbound>,
                value: &Self::GuestboundView<'_>,
            ) -> $crate::marshal_struct_internals::Result<()> {
                $(
                    <<$field_ty as $crate::marshal_struct_internals::Marshal>::Strategy>::encode_guestbound(
                        cx,
                        out_ptr.field($crate::marshal_struct_internals::ffi_offset!(
                            Self::Guestbound, $field_name,
                        )),
                        &value.$field_name,
                    )?;
                )*

                Ok(())
            }
        }
    )*};
}

// === Enum Marshalling === //

// Macro
pub mod marshal_enum_internals {
    pub use {
        crate::{FfiPtr, GuestInvokeContext, GuestMemoryContext, Marshal, Strategy},
        anyhow::{Result, bail},
        std::{
            clone::Clone,
            cmp::{Eq, Ord, PartialEq, PartialOrd},
            fmt::Debug,
            hash::Hash,
            marker::Copy,
        },
    };

    pub mod primitives {
        // From: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.intro
        pub use std::primitive::{i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize};
    }
}

#[macro_export]
macro_rules! marshal_enum {
    ($(
        $(#[$($item_meta:tt)*])*
        $item_vis:vis enum $item_name:ident : $repr:ident {
            $(
                $(#[$($field_meta:tt)*])*
                $variant_name:ident $(= $variant_val:expr)?
            ),+
            $(,)?
        }
    )*) => {$(
        #[derive(
            $crate::marshal_enum_internals::Debug,
            $crate::marshal_enum_internals::Copy,
            $crate::marshal_enum_internals::Clone,
            $crate::marshal_enum_internals::Hash,
            $crate::marshal_enum_internals::Eq,
            $crate::marshal_enum_internals::PartialEq,
            $crate::marshal_enum_internals::Ord,
            $crate::marshal_enum_internals::PartialOrd,
        )]
        $(#[$($item_meta:tt)*])*
        #[repr($repr)]
        $item_vis enum $item_name {
            $($variant_name $(= $variant_val)?,)*
        }

        impl $crate::marshal_enum_internals::Marshal for $item_name {
            type Strategy = Self;
        }

        impl $crate::marshal_enum_internals::Strategy for $item_name {
            type Hostbound<'a> = Self;
            type HostboundView = Self;
            type Guestbound = Self;
            type GuestboundView<'a> = Self;

            #[allow(non_upper_case_globals)]
            fn decode_hostbound(
                cx: &(impl ?Sized + $crate::marshal_enum_internals::GuestMemoryContext),
                ptr: $crate::marshal_enum_internals::FfiPtr<Self>,
            ) -> $crate::marshal_enum_internals::Result<Self> {
                $(
                    const $variant_name: $crate::marshal_enum_internals::primitives::$repr
                        = $item_name::$variant_name as $crate::marshal_enum_internals::primitives::$repr;
                )*
                match *ptr.cast::<$crate::marshal_enum_internals::primitives::$repr>().read(cx)? {
                    $($variant_name => Ok(Self::$variant_name),)*
                    _ => $crate::marshal_enum_internals::bail!("unknown enum variant"),
                }
            }

            fn encode_guestbound(
                cx: &mut impl $crate::marshal_enum_internals::GuestInvokeContext,
                out_ptr: $crate::marshal_enum_internals::FfiPtr<Self>,
                value: &Self,
            ) -> $crate::marshal_enum_internals::Result<()> {
                *out_ptr.cast::<$crate::marshal_enum_internals::primitives::$repr>().write(cx)?
                    = *value as $crate::marshal_enum_internals::primitives::$repr;

                Ok(())
            }
        }
    )*};
}

// === Tagged Union Marshalling === //

// Macro
pub mod marshal_tagged_union_internals {
    pub use {
        crate::{
            FfiPtr, GuestInvokeContext, GuestMemoryContext, GuestboundOf, GuestboundVariant,
            GuestboundViewVariant, HostboundOf, HostboundVariant, HostboundViewVariant,
            MarkerVariant, Marshal, Strategy, VariantSelector, ffi_offset,
        },
        anyhow::{Result, bail},
    };

    pub mod primitives {
        // From: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.intro
        pub use std::primitive::{i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize};
    }

    #[repr(C)]
    pub struct CPair<D, V> {
        pub discriminant: D,
        pub value: V,
    }
}

#[macro_export]
macro_rules! marshal_tagged_union {
    ($(
        $(#[$($item_meta:tt)*])*
        $item_vis:vis enum $item_name:ident : $repr:ident {
            $(
                $(#[$($field_meta:tt)*])*
                $variant_name:ident($variant_ty:ty)
            ),+
            $(,)?
        }
    )*) => {$(
        $(#[$($item_meta:tt)*])*
        // Represent the enum as a union of `(tag, value)` variants.
        // See: https://github.com/rust-lang/rfcs/blob/2ad779b99015e2a609ef416877485604d4799069/text/2195-really-tagged-unions.md#guide-level-explanation
        #[repr($repr)]
        $item_vis enum $item_name<
            V: $crate::marshal_tagged_union_internals::VariantSelector = $crate::marshal_tagged_union_internals::MarkerVariant
        > {
            $(
                $variant_name(<V as $crate::marshal_tagged_union_internals::VariantSelector>::Output<
                    <$variant_ty as $crate::marshal_tagged_union_internals::Marshal>::Strategy
                >),
            )*
        }

        impl $crate::marshal_tagged_union_internals::Marshal for $item_name {
            type Strategy = Self;
        }

        impl $crate::marshal_tagged_union_internals::Strategy for $item_name {
            type Hostbound<'a> = $item_name<$crate::marshal_tagged_union_internals::HostboundVariant<'a>>;
            type HostboundView = $item_name<$crate::marshal_tagged_union_internals::HostboundViewVariant>;
            type Guestbound = $item_name<$crate::marshal_tagged_union_internals::GuestboundVariant>;
            type GuestboundView<'a> = $item_name<$crate::marshal_tagged_union_internals::GuestboundViewVariant<'a>>;

            fn decode_hostbound(
                cx: &(impl ?Sized + $crate::marshal_tagged_union_internals::GuestMemoryContext),
                ptr: $crate::marshal_tagged_union_internals::FfiPtr<Self::Hostbound<'static>>,
            ) -> $crate::marshal_tagged_union_internals::Result<Self::HostboundView> {
                let variant = *ptr.cast::<$crate::marshal_tagged_union_internals::primitives::$repr>().read(cx)?;
                let counter = 0;

                $(
                    if variant == counter {
                        type VariantTy = $crate::marshal_tagged_union_internals::CPair<
                            $crate::marshal_tagged_union_internals::primitives::$repr,
                            $crate::marshal_tagged_union_internals::HostboundOf<'static, $variant_ty>,
                        >;

                        let field = ptr
                            .cast::<VariantTy>()
                            .field($crate::marshal_tagged_union_internals::ffi_offset!(VariantTy, value));

                        return Ok(Self::HostboundView::$variant_name(
                            <<$variant_ty as $crate::marshal_tagged_union_internals::Marshal>::Strategy>::decode_hostbound(
                                cx,
                                field,
                            )?,
                        ))
                    }

                    let counter = counter + 1;
                )*

                $crate::marshal_tagged_union_internals::bail!("unknown enum variant")
            }

            fn encode_guestbound(
                cx: &mut impl $crate::marshal_tagged_union_internals::GuestInvokeContext,
                out_ptr: $crate::marshal_tagged_union_internals::FfiPtr<Self::Guestbound>,
                value: &Self::GuestboundView<'_>,
            ) -> $crate::marshal_tagged_union_internals::Result<()> {
                let counter = 0;

                $(
                    if let Self::GuestboundView::$variant_name(value) = value {
                        type VariantTy = $crate::marshal_tagged_union_internals::CPair<
                            $crate::marshal_tagged_union_internals::primitives::$repr,
                            $crate::marshal_tagged_union_internals::GuestboundOf<$variant_ty>,
                        >;

                        *out_ptr.cast::<$crate::marshal_tagged_union_internals::primitives::$repr>().write(cx)? = counter;

                        let field = out_ptr
                            .cast::<VariantTy>()
                            .field($crate::marshal_tagged_union_internals::ffi_offset!(VariantTy, value));

                        <<$variant_ty as $crate::marshal_tagged_union_internals::Marshal>::Strategy>::encode_guestbound(
                            cx,
                            field,
                            value,
                        )?;

                        return Ok(());
                    }

                    let counter = counter + 1;
                )*

                unreachable!()
            }
        }
    )*};
}
