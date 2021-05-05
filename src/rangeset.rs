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
        assert!(idx < self.in_use as usize, "Index out of bounds.");

        // Copy the deleted range to the end of the list.
        for ii in idx..self.in_use as usize - 1 {
            self.ranges.swap(ii, ii+1);
        }

        // Decrement the number of valid ranges
        self.in_use -= 1;
        
    }

    /// Insert a new range into this RangeSet.
    /// If the range overlaps with an existing range, then the range will
    /// be merged. If the range has no overlap with an existing range then
    /// it will simply be added to the set. 
    pub fn insert(&mut self, mut range: Range) {
        assert!(range.start <= range.end, "Invalid range shape.");

        // Outside loop forever until we run out of merges with existing
        // ranges.
        'try_merges: loop {
            for ii in 0..self.in_use as usize {
                let ent = self.ranges[ii];

                // Check for overlap with an existing range.
                // Note that we do a saturated add of one to each range.
                // This is done so that two ranges that are 'touching' but
                // not overlapping will be combined.
                if overlaps(
                    Range {
                        start: range.start,
                        end: range.end.saturating_add(1),
                    },
                    Range {
                        start: ent.start,
                        end: ent.end.saturating_add(1)
                    }
                ).is_none() {
                    continue;
                }
                // There was an overlap with an existing range. Make this range
                // a combination of the existing ranges.
                range.start = cmp::min(range.start, ent.start);
                range.end = cmp::max(range.end, ent.end);

                // Delete the old range, as the new one is now all inclusive
                self.delete(ii);

                // Start over looking for merges
                continue 'try_merges;
            }

            break;
        }
        
        assert!((self.in_use as usize) < self.ranges.len(),
            "Too many entries in RangeSet on insert.");

        // Add the new range to the end
        self.ranges[self.in_use as usize] = range;
        self.in_use += 1;
    }

    /// Remove `range` from the RangeSet
    /// Any range in the RangeSet which overlaps with `range` will be trimmed
    /// such that there is no more overlap. If the results in a range in
    /// the set become empty, the range will be removed entirely from the set.
    pub fn remove(&mut self, range: Range) {
        assert!(range.start <= range.end, "Invalid range shape");

        'try_subtraction: loop {
            for ii in 0..self.in_use as usize {
                let ent = self.ranges[ii];

                // If there is no overlap, there is nothing to do with this
                // range.
                if overlaps(range, ent).is_none() {
                    continue;
                }

                // If this entry is entirely contained by the range to remove
                // we can just delete it.
                if contains(ent, range) {
                    self.delete(ii);
                    continue 'try_subtraction;
                }

                // At this point we know there is partial overlap. This means
                // we need to adjust the size of the current range and 
                // potentially insert a new entry if the entry is split in two.
                if range.start <= ent.start {
                    // If the overlap is on the low end of the range, adjust 
                    // the start of the range to the end of the range we want
                    // to remove.
                    self.ranges[ii].start = range.end.saturating_add(1);
                } else if range.end >= ent.end {
                    // If the overlap is on the high end of the range, adjust
                    // the end of the range to the start of the range we want
                    // to remove.
                    self.ranges[ii].end = range.start.saturating_sub(1);
                }
                else {
                    // If the range to remove fits inside of the range then we
                    // need to split it into two ranges.
                    self.ranges[ii].start = range.end.saturating_add(1);
                    assert!((self.in_use as usize) < self.ranges.len(),
                        "Too many entries in RangeSet on split.");
                    
                        self.ranges[self.in_use as usize] = Range {
                            start: ent.start,
                            end: range.start.saturating_sub(1),
                        };
                        self.in_use += 1;
                        continue 'try_subtraction;
                }

            }
            
            break;
        }
    } 

    /// Subtracts a `RangeSet` from `self`
    pub fn subtract(&mut self, rs: &RangeSet) {
        for &ent in rs.entries() {
            self.remove(ent)
        }
    }

    /// Compute the size of the range covered by this rangeset
    pub fn sum(&self) -> Option<u64> {
        self.entries().iter().try_fold( 0u64, |acc, x| {
            Some(acc + (x.end - x.start).checked_add(1)?)
        })
    }

    /// Allocate `size` bytes of memory with `align` requirement for alignment
    pub fn allocate(&mut  self, size: u64, align: u64) -> Option<usize> {
        // Allocate anywhere from the `RangeSet`
        self.allocate_prefer(size, align, None)
    }

    

}