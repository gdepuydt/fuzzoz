use core::intrinsics::wrapping_sub;

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
pub unsafe extern fn memcpy(dest: *mut u8, src: *const u8, n: usize) 
    -> *mut u8 {
        asm!("rep movsb",
            inout("rcx") n => _,
            inout("rdi") dest => _,
            inout("rsi") src => _);

        dest
}

#[no_mangle]
pub unsafe extern fn memmove(dest: *mut u8, src: *const u8, mut n: usize) 
    -> *mut u8 {

        // Determine if the dest comes after the src and if there's overlap
        if (dest as usize) > (src as usize) &&
            (src as usize).wrapping_add(n) > (dest as usize) {
                // There is at least one byte of overlap and the src is prior 
                // to the dest

                // Compute the delta between the source and the dest
                let delta = (dest as usize) - (src as usize);

                // if the delta is small, copy in reverse
                if delta < 64 {
                    // 8 byte align dest with one byte copies
                    while n != 0 && (dest as usize).wrapping_add(n) & 0x7 != 0 {
                        n = n.wrapping_sub(1);
                        *dest.offset(n as isize) = *src.offset(n as isize);
                    }

                    // when the dest is aligned, do a reverse copy 8-bytes at a time
                    while n >=8 {
                        n = n.wrapping_sub(8);

                        // Read the value to copy
                        let val = core::ptr::read_unaligned(
                            src.offset(n as isize) as *const u64
                        );

                        // Write out the value
                        core::ptr::write(dest.offset(n as isize) as *mut u64, val);
                    }

                    // Copy the remainder
                    while n != 0 {
                        n = n.wrapping_sub(1);
                        *dest.offset(n as isize) = *src.offset(n as isize);
                    }

                    return dest;
                }

                // Copy the non-overlapping tail parts while there are overhang
                // sized chunks
                while n >= delta {
                    // Update the length remaining
                    n = n.wrapping_sub(delta);

                    let src = src.offset(n as isize);
                    let dest = dest.offset(n as isize);
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
pub unsafe extern fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    asm!("rep stosb",
        inout("rcx") n => _,
        inout("rdi") s => _,
        in("eax") c as u32

    );

    s
}

#[no_mangle]
pub unsafe extern fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    let mut ii = 0;

    while ii <  n {
        let a = s1.offset(ii as isize);
        let b = s2.offset(ii as isize);
        if a != b {
            return (a as i32).wrapping_sub(b as i32);
        }

        ii = ii.wrapping_add(1);
    }
    0
}



