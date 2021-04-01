#![feature(asm, panic_info_message, core_intrinsics)]
#![allow(clippy::print_with_newline, non_snake_case, dead_code)]
#![no_std]
#![no_main]

#[macro_use]
mod print;
mod core_requirements;
mod efi;
use core::panic::PanicInfo;
use efi::{EfiHandle, EfiStatus, EfiSystemTable};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    print!("!!! PANIC !!!\n");
    if let Some(location) = info.location() {
        print!(
            "{}:{}:{}\n",
            location.file(),
            location.line(),
            location.column()
        );
    }

    if let Some(message) = info.message() {
        print!("{}\n", message);
    }
    loop {
        unsafe { asm!("hlt") }
    }
}

#[no_mangle]
extern "C" fn efi_main(_image_handle: EfiHandle, system_table: *mut EfiSystemTable) -> EfiStatus {
    // First,  register the EFI systen table in a global so we can use it
    // in other places such as a `print!` macro
    unsafe { efi::register_system_table(system_table); }

    panic!("Moose!");
}