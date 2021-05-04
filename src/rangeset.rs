//! Module which provides `Rangeset` which contains non-overlapping sets of `u64` inclusive ranges.
//! The `Rangeset` can be used to insert or remove ranges of `u64`s and thus is 
//! very useful for physical memory management 

use core::cmp;
/// An inclusive range. We do not use `RangeInclusive` as it does not implement
/// a `Copy`.
#[derive(Clone, Copy, Debug)]
pub struct Range {
    
    /// Start of the range (inclusive)
    pub start: u64,
    
    /// End of the range (inclusive)
    pub end: u64,
}
/// A set of non-overlapping inclusive `u64` ranges. 
#[derive(Clone, Copy)]
pub struct RangeSet {
    
    /// Fixed array of `ranges`.
    ranges: [Range; 256],
    
    /// Number of in use entries in `ranges`.
    in_use: usize,
}

impl RangeSet {
    
    /// Create a new empty RangeSet.
    pub const fn new() -> RangeSet {
        RangeSet {
            ranges: [ Range{ start: 0, end: 0} ; 256],
            in_use: 0,
        }
    }

    /// Get al the entries in the RangeSet as slice.
    pub fn entries(&self) -> &[Range] {
        &self.ranges[..self.in_use]
    }

    /// Delete the `Range` contained in the RangeSet at `idx`.
    pub fn delete(&mut self, idx: usize) {
        todo!();
    }

    /// Insert a new range into this RangeSet.
    pub fn insert(&mut self, mut range: Range) {
        todo!();
    }

}