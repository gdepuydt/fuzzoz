#![feature(asm, panic_info_message, core_intrinsics, bool_to_option)]
#![allow(clippy::print_with_newline, non_snake_case, dead_code)]
#![feature(arbitrary_enum_discriminant)]
#![no_std]
#![no_main]

#[macro_use]
mod print;
mod acpi;
mod core_requirements;
mod efi;
mod mm;
use core::panic::PanicInfo;
use efi::{EfiHandle, EfiSystemTablePtr, EfiStatusCode};

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
extern "C" fn efi_main(image_handle: EfiHandle, system_table: EfiSystemTablePtr) -> EfiStatusCode {
    // First,  register the EFI system table in a global so we can use it
    // in other places such as a `print!` macro
    unsafe { system_table.register() };

    unsafe {
        acpi::init().expect("Failed to initialize ACPI");
    };

    efi::get_memory_map(image_handle).expect("Failed to get EFI Memory Map");

    panic!("Moose!");
}
