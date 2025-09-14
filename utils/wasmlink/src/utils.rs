use std::mem;

cfgenius::define! {
    pub is_wasm = cfg(target_arch = "wasm32");
}

pub const fn align_of_u32<T>() -> u32 {
    const {
        let align = mem::align_of::<T>() as u64;

        if align > u32::MAX as u64 {
            panic!("alignment is too large for guest")
        }

        align as u32
    }
}

pub const fn size_of_u32<T>() -> u32 {
    const {
        let align = mem::size_of::<T>() as u64;

        if align > u32::MAX as u64 {
            panic!("size is too large for guest")
        }

        align as u32
    }
}

pub const fn guest_usize_to_u32(val: usize) -> u32 {
    cfgenius::cond! {
        if macro(is_wasm) {
            val as u32
        } else {
            _ = val;

            unimplemented!();
        }
    }
}

macro_rules! impl_tuples {
	// Internal
	(
		$target:path : []
		$(| [
			$({$($pre:tt)*})*
		])?
	) => { /* terminal recursion case */ };
	(
		$target:path : [
			{$($next:tt)*}
			// Remaining invocations
			$($rest:tt)*
		] $(| [
			// Accumulated arguments
			$({$($pre:tt)*})*
		])?
	) => {
		$target!(
			$($($($pre)*,)*)?
			$($next)*
		);
		$crate::utils::impl_tuples!(
			$target : [
				$($rest)*
			] | [
				$($({$($pre)*})*)?
				{$($next)*}
			]
		);
	};

	// Public
	($target:path; no_unit) => {
		$crate::utils::impl_tuples!(
			$target : [
				{A: 0}
				{B: 1}
				{C: 2}
				{D: 3}
				{E: 4}
				{F: 5}
				{G: 6}
				{H: 7}
				{I: 8}
				{J: 9}
				{K: 10}
				{L: 11}
			]
		);
	};
	($target:path; only_full) => {
		$target!(
			A:0,
			B:1,
			C:2,
			D:3,
			E:4,
			F:5,
			G:6,
			H:7,
			I:8,
			J:9,
			K:10,
			L:11
		);
	};
	($target:path) => {
		$target!();
		$crate::utils::impl_tuples!($target; no_unit);
	};
}

pub(crate) use impl_tuples;
