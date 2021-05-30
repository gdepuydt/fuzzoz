//! Physical memory management for the OS

use core::mem::size_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(pub u64);

impl PhysAddr {
    /// Read a `T` from physical memory address `paddr`
    #[inline]
    pub unsafe fn read<T>(&self) -> T {
        core::ptr::read(self.0 as *const T)
    }

    /// Read an unaligned `T` from physical memory address `paddr`.
    #[inline]
    pub unsafe fn read_unaligned<T>(&self) -> T {
        core::ptr::read_unaligned(self.0 as *const T)
    }
}

/// A consumeable slice of physical memory at `addr` for `size` bytes.
pub struct PhysSlice {
    addr: PhysAddr, 
    len: usize,
}

impl PhysSlice {
    /// Create a new slice to physical memory.
    pub unsafe fn new(addr: PhysAddr, size: usize) -> Self {
        PhysSlice {
            addr, 
            len: size
        }
    }

    /// Get the remaining length of the slice.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Discard bytes from the slice by updating the pointer and length.
    pub fn discard(&mut self, bytes: usize) -> Result<(), ()> {
        if self.len >= bytes {
            // Update the pointer and length.
            (self.addr).0 += bytes as u64; 
            self.len -= bytes;
            Ok(())
        } else {
            Err(())
        }
    }

    /// Read a `T` from the slice, updating the pointer.
    pub unsafe fn consume<T>(&mut self) -> Result<T, ()> {
        // Make sure we have enough data to consume.
        if self.len < size_of::<T>() {
            return Err(());
        }

        // Read the actual data.
        let data = self.addr.read_unaligned::<T>();

        // Update the pointer and length.
        (self.addr).0 += size_of::<T>() as u64; 
        self.len -= size_of::<T>();

        Ok(data)
    }
}