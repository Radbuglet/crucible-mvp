use num_traits::PrimInt;

pub fn pack_bitmask<T: PrimInt, const N: usize>(parts: [(usize, T); N]) -> T {
    let mut accum = T::zero();

    for (bit_count, part) in parts.into_iter().rev() {
        accum = accum << bit_count;
        accum = accum | (part & ((T::one() << bit_count) - T::one()));
    }

    accum
}
