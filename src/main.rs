#![feature(asm)]
#![no_std]
#![no_main]

mod core_requirements;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop{}
}

#[no_mangle]
extern fn efi_main() {
}
