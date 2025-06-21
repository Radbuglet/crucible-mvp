pub fn align_to_pow_2(val: u32, align: u32) -> Option<u32> {
    debug_assert!(align.is_power_of_two());

    Some(val.checked_add(align - 1)? & !(align - 1))
}
