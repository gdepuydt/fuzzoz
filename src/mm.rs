//! Memory management routines

use core::mem::size_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(pub u64);

/// A consumeable slice of physical memory
pub struct PhysSlice(PhysAddr, usize);

impl PhysSlice {
    /// Create a new slice to physical memory
    pub unsafe fn new( addr: PhysAddr, size: usize) -> Self {
        PhysSlice(addr,size)
    }

    /// Read a `T` from the slice, updating the pointer
    pub unsafe fn consume<T>(&mut self) -> Result<T, ()> {
        // Make sure we have enough data to consume
        if self.1 < size_of::<T>() {
            return Err(());
        }

        // Read the actual data
        let data = read_phys_unaligned::<T>(self.0); 

        // Compute the updated pointer
        let new_ptr = self.0.0.checked_add(size_of::<T>() as u64).ok_or(())?;

        // Read the memory
        todo!()
    }
}

/// Read a `T` from physical memory address `paddr`
#[inline]
pub unsafe fn read_phys<T>(paddr: PhysAddr) -> T {
    core::ptr::read(paddr.0 as *const T)
}

/// Read an unaligned `T` from physical memory address `paddr`
#[inline]
pub unsafe fn read_phys_unaligned<T>(paddr: PhysAddr) -> T {
    core::ptr::read_unaligned(paddr.0 as *const T)
}
