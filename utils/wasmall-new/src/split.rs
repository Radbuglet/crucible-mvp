use std::{ops::Range, slice};

use smallvec::SmallVec;

// === SplitIter === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
struct Split {
    position: usize,
    include_left: bool,
}

#[derive(Debug)]
struct SplitIter<'a> {
    /// The remaining [`Split`] events to process.
    splits: slice::Iter<'a, Split>,

    /// The data we're splitting.
    data: &'a [u8],

    last_range_end: usize,
}

impl<'a> SplitIter<'a> {
    pub fn new(splits: &'a [Split], data: &'a [u8]) -> Self {
        Self {
            splits: splits.iter(),
            data,
            last_range_end: 0,
        }
    }
}

impl Iterator for SplitIter<'_> {
    type Item = SmallVec<[Range<usize>; 1]>;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

// === Driver === //
