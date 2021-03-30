use core::sync::atomic::{AtomicPtr, Ordering};

static EFI_SYSTEM_TABLE: AtomicPtr<EfiSystemTable> = 
AtomicPtr::new(core::ptr::null_mut());

pub unsafe fn register_system_table(system_table: *mut EfiSystemTable) {
    EFI_SYSTEM_TABLE.compare_and_swap(core::ptr::null_mut(), system_table, Ordering::SeqCst);
}