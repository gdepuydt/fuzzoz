// was temporarily removed
#[inline(always)]
#[cfg(target_arch = "x86_64")]
unsafe fn memcpy_int(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    asm!("rep movsb",
        inout("rcx") n => _,
        inout("rdi") dest => _,
        inout("rsi") src => _);

    dest
}

#[no_mangle]
#[cfg(target_arch = "x86_64")]
unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    asm!("rep movsb",
            inout("rcx") n => _,
            inout("rdi") dest => _,
            inout("rsi") src => _);

    dest
}


#[no_mangle]
#[cfg(not(target_arch = "x86_64"))]
unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut ii = 0;

    while ii < n {
        let dest = dest.offset(ii as isize);
        let src = src.offset(ii as isize);
        core::ptr::write(dest, core::ptr::read(src));
        ii += 1;
    }

    dest
}



#[no_mangle]
unsafe extern "C" fn memmove(dest: *mut u8, src: *const u8, mut n: usize) -> *mut u8 {
    // Determine if the dest comes after the src and if there's overlap
    if (dest as usize) > (src as usize) && (src as usize).wrapping_add(n) > (dest as usize) {
        // There is at least one byte of overlap and the src is prior
        // to the dest

        // Compute the delta between the source and the dest
        let delta = (dest as usize) - (src as usize);

        // if the delta is small, copy in reverse
        if delta < 64 {
            // 8 byte align dest with one byte copies
            while n != 0 && (dest as usize).wrapping_add(n) & 0x7 != 0 {
                n = n.wrapping_sub(1);
                core::ptr::write(dest.add(n), core::ptr::read(src.add(n)));
            }

            // when the dest is aligned, do a reverse copy 8-bytes at a time
            while n >= 8 {
                n = n.wrapping_sub(8);

                // Read the value to copy
                let val = core::ptr::read_unaligned(src.add(n) as *const u64);

                // Write out the value
                core::ptr::write(dest.add(n) as *mut u64, val);
            }

            // Copy the remainder
            while n != 0 {
                n = n.wrapping_sub(1);
                core::ptr::write(dest.add(n), core::ptr::read(src.add(n)));
            }

            return dest;
        }

        // Copy the non-overlapping tail parts while there are overhang
        // sized chunks
        while n >= delta {
            // Update the length remaining
            n = n.wrapping_sub(delta);

            let src = src.add(n);
            let dest = dest.add(n);
            memcpy(dest, src, delta);
        }

        // check if we copied everything
        if n == 0 {
            return dest;
        }

        // At this point n < delta so we are in a non-overlapping region
    }

    // Just copy the remaining bytes forward one by one
    memcpy(dest, src, n)
}

// Fill memory with a constant
#[no_mangle]
#[cfg(target_arch = "x86_64")]
unsafe extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    asm!("rep stosb",
        inout("rcx") n => _,
        inout("rdi") s => _,
        in("eax") c as u32

    );

    s
}

// Fill memory with a constant
#[no_mangle]
#[cfg(not(target_arch = "x86_64"))]
unsafe extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    let mut ii = 0;

    while ii < n {
        let s = s.offset(ii as isize);
        core::ptr::write(s, c as u8);
        ii += 1;
    }

    s
}


#[no_mangle]
unsafe extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    let mut ii = 0;

    while ii < n {
        let a = core::ptr::read(s1.add(ii));
        let b = core::ptr::read(s2.add(ii));
        if a != b {
            return (a as i32).wrapping_sub(b as i32);
        }

        ii = ii.wrapping_add(1);
    }
    0
}
