#[doc(hidden)]
pub mod delegate_internals {
    pub use {
        derive_where,
        smallbox::{SmallBox, smallbox},
        std::{
            convert::From,
            ops::{FnMut, FnOnce},
            option::Option::Some,
        },
    };

    #[doc(hidden)]
    pub trait NewFnMutHelper<T> {
        fn __new_mut_ctor(value: T) -> Self;
    }
}

#[macro_export]
macro_rules! delegate_once {
    ($(
        $(#[$($attr:tt)*])*
        $vis:vis fn $name:ident
            $([<
                $($struct_lt:lifetime),* $(,)?
                $($struct_generic:ident),* $(,)?
            > $(where $($clause:tt)*)?])?
            $(<$($lt:lifetime),*$(,)?>)?
            (
                $($arg:ident: $ty:ty),*
                $(,)?
            )
            $(-> $ret:ty)?;
    )*) => {$(
        #[$crate::lang::delegate_internals::derive_where::derive_where(Debug)]
        #[derive_where(crate = $crate::lang::delegate_internals::derive_where)]
        $(#[$($attr)*])*
        $vis struct $name $(<$($struct_lt,)* $($struct_generic,)*> $(where $($clause)*)?)? {
            #[derive_where(skip)]
            #[allow(dead_code)]
            inner: $crate::lang::delegate_internals::SmallBox<
                dyn $(for<$($lt),*>)? $crate::lang::delegate_internals::FnMut($($ty),*) $(-> $ret)?,
                [usize; 2],
            >,
        }

        impl <
            $($($struct_lt,)*)? $($($struct_generic,)*)?
            __F: 'static + $(for<$($lt),*>)? $crate::lang::delegate_internals::FnMut($($ty),*) $(-> $ret)?,
        > $crate::lang::delegate_internals::NewFnMutHelper<__F> for $name $(<$($struct_lt,)* $($struct_generic,)*>
        $(where $($clause)*)?)?
        {
            fn __new_mut_ctor(f: __F) -> Self {
                Self {
                    inner: $crate::lang::delegate_internals::smallbox!(f),
                }
            }
        }

        impl <
            $($($struct_lt,)*)? $($($struct_generic,)*)?
            __F: 'static + $(for<$($lt),*>)? $crate::lang::delegate_internals::FnMut($($ty),*) $(-> $ret)?,
        > $crate::lang::delegate_internals::From<__F> for $name $(<$($struct_lt,)* $($struct_generic,)*>
        $(where $($clause)*)?)?
        {
            fn from(f: __F) -> Self {
                Self::new(f)
            }
        }

        #[allow(dead_code)]
        impl $(<$($struct_lt,)* $($struct_generic,)*>)? $name $(<$($struct_lt,)* $($struct_generic,)*>
        $(where $($clause)*)?)?
        {
            pub fn new(f: impl 'static + $(for<$($lt),*>)? $crate::lang::delegate_internals::FnOnce($($ty),*) $(-> $ret)?) -> Self {
                let mut f = $crate::lang::delegate_internals::Some(f);
                <Self as $crate::lang::delegate_internals::NewFnMutHelper<_>>::__new_mut_ctor(move |$($arg),*| f.take().unwrap()($($arg),*))
            }

            pub fn call$(<$($lt,)*>)?(mut self, $($arg: $ty,)*) $(-> $ret)? {
                (self.inner)($($arg,)*)
            }
        }
    )*};
}

pub use delegate_once;
