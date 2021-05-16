//! Module which provides `Rangeset` which contains non-overlapping sets of `u64` inclusive ranges.
//! The `Rangeset` can be used to insert or remove ranges of `u64`s and thus is 
//! very useful for physical memory management. 


/// A `Result` type which wraps a `RangeSet` error.
type Result<T> = core::result::Result<T, Error>;
/// Errors associated with `RangeSet` operations.
#[derive(Debug)]
pub enum Error {
    
    /// An 'out of bounds' index was specified.
    InvalidIndex,
    
    /// A range was specified with an invalid shape (Start > End).
    InvalidRange,

    /// An operation was performed on the `RangeSet` but there was no more space 
    /// in the fixed allocation of ranges.
    OutOfEntries,

    /// A request for a free range failed due not having a free range with 
    /// the requested size and alignment.
    OutOfMemory,

    /// Zero sized allocations are not supported.
    ZeroSizeAllocation,

    /// The alignment specified was not a power of 2 or was zero.
    InvalidAlignment,


}


use core::cmp;
/// An inclusive range. We do not use `RangeInclusive` as it does not implement
/// a `Copy`.
#[derive(Clone, Copy, Debug)]
pub struct Range {
    
    /// Start of the range (inclusive).
    pub start: u64,
    
    /// End of the range (inclusive).
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
    pub fn delete(&mut self, idx: usize) -> Result<()> {
        // Make sure we're deleting a valid index.
        if idx >= self.in_use {
            return Err(Error::InvalidIndex);
        }
        
        assert!(idx < self.in_use as usize, "Index out of bounds.");

        // Copy the deleted range to the end of the list.
        self.ranges.swap(idx, self.in_use - 1);

        // Decrement the number of valid ranges
        self.in_use -= 1;

        Ok(())
        
    }

    /// Insert a new range into this RangeSet.
    /// If the range overlaps with an existing range, then the range will
    /// be merged. If the range has no overlap with an existing range then
    /// it will simply be added to the set. 
    pub fn insert(&mut self, mut range: Range) -> Result<()> {
        
        // Check the range
        if range.end < range.start {
            return Err(Error::InvalidRange);
        }
        
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
                // There was an overlap with an existing range. Make this range.
                // a combination of the existing ranges.
                range.start = cmp::min(range.start, ent.start);
                range.end = cmp::max(range.end, ent.end);

                // Delete the old range, as the new one is now all inclusive.
                self.delete(ii)?;

                // Start over looking for merges.
                continue 'try_merges;
            }

            break;
        }
        

        
        // Add the new range to the end.
        if let Some(ent) = self.ranges.get_mut(self.in_use) {
            *ent = range;
            self.in_use += 1;
            Ok(())
        } else {
            // If we deleted anything above it's impossible for this error to
            // occur as we know there is at least space for one entry. Thus we don't
            // have to worry about restored ranges from above.
            Err(Error::OutOfEntries)
        }
    }

    /// Remove `range` from the RangeSet
    /// Any range in the RangeSet which overlaps with `range` will be trimmed
    /// such that there is no more overlap. If the results in a range in
    /// the set become empty, the range will be removed entirely from the set.
    pub fn remove(&mut self, range: Range) -> Result<()> {
        
        // Check the range
        if range.end < range.start {
            return Err(Error::InvalidRange);
        }

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
                    self.delete(ii)?;
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
                    
                    // Insert a new range for the tail.
                    if let Some(ent) = self.ranges.get_mut(self.in_use) {
                        *ent = Range {
                            start: ent.start,
                            end: range.start.saturating_sub(1),
                        };
                        self.in_use += 1; 
                    } else {
                        return Err(Error::OutOfEntries);
                    }
                        
                    continue 'try_subtraction;
                }

            }
            // No more subtractions could be found.
            break;
        }
        Ok(())
    } 

    /// Subtracts a `RangeSet` from `self`
    pub fn subtract(&mut self, rs: &RangeSet) -> Result<()> {
        for &ent in rs.entries() {
            self.remove(ent)?;
        }
        Ok(())
    }

    /// Compute the size of the range covered by this rangeset
    pub fn sum(&self) -> Option<u64> {
        self.entries().iter().try_fold( 0u64, |acc, x| {
            Some(acc + (x.end - x.start).checked_add(1)?)
        })
    }

    /// Allocate `size` bytes of memory with `align` requirement for alignment
    pub fn allocate(&mut  self, size: u64, align: u64) -> Result<usize> {
        // Allocate anywhere from the `RangeSet`
        self.allocate_prefer(size, align, None)
    }

    
    /// Allocate `size` bytes of memory with `align` requirements for alignment
    /// Preferring to allocate from the `region`. If an allocation cannot be 
    /// satisfied from `regions` the allocation will come from whatever is next
    /// best. If `regions` is `None`, the allocation will be satisfied 
    /// from anywhere. This will be the core of our physical memory manager.
    pub fn allocate_prefer(&mut self, size: u64, align: u64,
                                regions: Option<&RangeSet>) -> Result<usize> {
        // Don't allow allocations of zero size
        if size == 0 {
            return Err(Error::ZeroSizeAllocation);
        }

        // Validate alignment is non-zero and a power of 2
        if align.count_ones() != 1 {
            return Err(Error::InvalidAlignment);
        }

        // Generate a mask for the specified alignment.
        let alignmask = align - 1;

        // Go through each memory range in the `RangeSet`.
        let mut allocation = None;
        'allocation_search: for ent in self.entries() {
            // Determine the number of bytes required for front padding to
            // satisfy alignment requirements
            let align_fix = (align - (ent.start & alignmask)) & alignmask;

            // Compute base and end of allocation as an inclusive range 
            // [base, end].
            let base = ent.start;
            let end = if let Some(end)  = base.checked_add(size - 1)
                .and_then(|x| x.checked_add(align_fix)) {
                    end
                } else {
                    // This range cannor satisfy this allocation as there was an
                    // overflow on the range.
                    continue;
                };


            /*// Validate the the allocation is addressable in the current 
            // processor state.
            if base > core::usize::MAX as u64 || 
                end > core::usize::MAX as u64 {
                    continue;
            }*/

            // Check that this entity has enough room to satisfy allocation
            if end > ent.end {
                continue;
            }
            
            // If there was a specific redion the caller wanted to use.
            if let Some(region) = regions {
                // Check if there is overlap with this region
                for &region in region.entries() {
                    if let Some(overlap) = overlaps(*ent, region) {
                        // Compute the rounded-up alignment from the
                        // overlapping region.
                        let align_overlap = (overlap.start.wrapping_add(alignmask)) & 
                            !alignmask;
                        if align_overlap >= overlap.start && 
                            align_overlap <= overlap.end &&
                            (overlap.end - align_overlap) >= (size - 1) {
                            
                            // Alignment did not cause and underflow AND 
                            // alignment did not cause exceeding the end AND
                            // amount of aligned overap can satisfy 
                            // the allocation.

                            // Compute the inclusive end of this proposed
                            // allocation.
                            let overlap_alc_end = align_overlap + (size - 1);
                            
                            /*// Make sure the allocation fits in the current
                            // addressable address space.
                            if align_overlap > core::usize::MAX as u64 ||
                                overlap_alc_end > core::usize::MAX as u64 {
                                    continue 'allocation_search;
                            }*/

                            // We know the allocation can be satisfied starting
                            // at `align_overlap`.
                            // Break out immediately as we prioritize NUMA over
                            // size.
                            allocation = Some((align_overlap, 
                                                overlap_alc_end,
                                                align_overlap as usize));
                            break 'allocation_search
                        }
                    }
                }
            }

            // Compute the "best" allocation size to date.
            let prev_size = allocation.map(|(base, end, _)| end - base);

            // If no previous allocation or the new allocation uses less memory
            // than the previous allocation
            if allocation.is_none() || prev_size.unwrap() > end - base {
                // Update the allocation to the new best size.
                allocation = Some((base, end, (base + align_fix) as usize));
            }
        }

        if let Some((base, end,ptr)) = allocation {
            // Remove this range from the available set.
            self.remove(Range {start: base, end})?;
            Ok(ptr)
        } else {
            // Could not satisfy allocation.
            Err(Error::OutOfMemory)
        }
    }
}

fn overlaps(mut a: Range, mut b: Range) -> Option<Range> {
    // Make sure range 'a' is always lowest to biggest.
    if a.start > a.end {
        core::mem::swap(&mut a.end, &mut a.start);
    }

    // Make sure range 'b' is always lowest to biggest.
    if b.start > b.end {
        core::mem::swap(&mut b.end, &mut b.start);
    }

    // Check if there is overlap.
    if a.start <= b.end && b.start <= a.end {
        Some (Range {
            start: core::cmp::max(a.start, b.start),
            end: core::cmp::min(a.end, b.end),
        })
    } else {
        None
    }
}

fn contains(mut a: Range, mut b: Range) -> bool {
    // Make sure range 'a' is always lowest to biggest.
    if a.start > a.end {
        core::mem::swap(&mut a.end, &mut a.start);
    }

    // Make sure range 'b' is always lowest to biggest.
    if b.start > b.end {
        core::mem::swap(&mut b.end, &mut b.start);
    }

    a.start >= b.start && a.end <= b.end
}

