use core::{
    sync::atomic::{AtomicPtr, Ordering},
    usize,
};

static EFI_SYSTEM_TABLE: AtomicPtr<EfiSystemTable> = AtomicPtr::new(core::ptr::null_mut());

pub unsafe fn register_system_table(system_table: *mut EfiSystemTable) -> EfiStatus {
    EFI_SYSTEM_TABLE
        .compare_exchange(
            core::ptr::null_mut(),
            system_table,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .unwrap();
    EfiStatus(0)
}

pub fn output_string(string: &str) {
    let system_table = EFI_SYSTEM_TABLE.load(Ordering::SeqCst);

    if system_table.is_null() {
        return;
    }

    let console_out = unsafe { (*system_table).console_out };

    // We are using UCS-2 and not UTF-16, as that's what UEFI uses.
    // Thus, we don't have to worry about 32-bit code points
    let mut tmp = [0u16; 32];
    let mut in_use = 0;

    for chr in string.encode_utf16() {
        if chr == b'\n' as u16 {
            tmp[in_use] = b'\r' as u16;
            in_use += 1;
        }

        tmp[in_use] = chr;
        in_use += 1;

        if in_use == (tmp.len() - 2) {
            tmp[in_use] = 0;

            unsafe {
                ((*(console_out)).output_string)(console_out, tmp.as_ptr());
            }

            in_use = 0;
        }
    }

    if in_use > 0 {
        tmp[in_use] = 0;
        unsafe {
            ((*(console_out)).output_string)(console_out, tmp.as_ptr());
        }
    }
}

pub fn get_memory_map(image_handle: EfiHandle) {
    let system_table = EFI_SYSTEM_TABLE.load(Ordering::SeqCst);

    if system_table.is_null() {
        return;
    }

    let mut memory_map = [0u8; 4 * 1024];

    let mut free_memory = 0u64;

    unsafe {
        let mut size = core::mem::size_of_val(&memory_map);

        let mut key = 0;
        let mut mdesc_size = 0;
        let mut mdesc_version = 0;

        let ret = ((*(*system_table).boot_services).get_memory_map)(
            &mut size,
            memory_map.as_mut_ptr(),
            &mut key,
            &mut mdesc_size,
            &mut mdesc_version,
        );

        assert!(ret.0 == 0, "Get memory map failed: {:?}", ret);

        for offset in (0..size).step_by(mdesc_size) {
            let entry = core::ptr::read_unaligned(
                memory_map[offset..].as_ptr() as *const EfiMemoryDescriptor
            );

            let typ: EfiMemoryType = entry.typ.into();

            if typ.avail_post_exit_boot_service() {
                free_memory += entry.number_of_pages * 4096;
            }
            /*
            print!(
                "{:016x} {:016x} {:?}\n",
                entry.physical_start,
                entry.number_of_pages * 4096,
                typ
            );*/
        }

        // Exit Boot serices
        let ret = ((*(*system_table).boot_services).exit_boot_services)(
            image_handle,
            key
        );

        assert!(ret.0 == 0, "Failed to exit boot services: {:?}", ret);

        // Kill the EFI system table
        EFI_SYSTEM_TABLE.store(core::ptr::null_mut(), Ordering::SeqCst);

    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct EfiHandle(usize);

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct EfiStatus(pub usize);

#[repr(C)]
struct EfiInputKey {
    scan_code: u16,
    unicode_char: u16,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
enum EfiMemoryType {
    ReservedMemoryType,
    LoaderCode,
    LoaderData,
    BootServiceCode,
    BootServiceData,
    RuntimeServiceCode,
    RuntimeServiceData,
    ConventionalMemory,
    UnusableMemory,
    ACPIReclaimMemory,
    ACPIMemoryNVS,
    MemoryMappedIO,
    MemoryMappedIOPortSpace,
    PalCode,
    PersistentMemory,
    Invalid,
}

impl EfiMemoryType {
    fn avail_post_exit_boot_service(&self) -> bool {
        matches!(self, EfiMemoryType::BootServiceCode
            | EfiMemoryType::BootServiceData
            | EfiMemoryType::ConventionalMemory
            | EfiMemoryType::PersistentMemory)
    }
}

impl From<u32> for EfiMemoryType {
    fn from(val: u32) -> Self {
        match val {
            0 => EfiMemoryType::ReservedMemoryType,
            1 => EfiMemoryType::LoaderCode,
            2 => EfiMemoryType::LoaderData,
            3 => EfiMemoryType::BootServiceCode,
            4 => EfiMemoryType::BootServiceData,
            5 => EfiMemoryType::RuntimeServiceCode,
            6 => EfiMemoryType::RuntimeServiceData,
            7 => EfiMemoryType::ConventionalMemory,
            8 => EfiMemoryType::UnusableMemory,
            9 => EfiMemoryType::ACPIReclaimMemory,
            10 => EfiMemoryType::ACPIMemoryNVS,
            11 => EfiMemoryType::MemoryMappedIO,
            12 => EfiMemoryType::MemoryMappedIOPortSpace,
            13 => EfiMemoryType::PalCode,
            14 => EfiMemoryType::PersistentMemory,
            _ => EfiMemoryType::Invalid,
        }
    }
}

#[repr(C)]
struct EfiTableHeader {
    signature: u64,
    revision: u32,
    header_size: u32,
    crc32: u32,
    reserved: u32,
}

#[derive(Clone, Copy, Default, Debug)]
#[repr(C)]
struct EfiMemoryDescriptor {
    typ: u32,
    // Must be alligined on a 4KiB boundary, not above 0xfffffffffffff000;
    physical_start: u64,
    // Must be alligined on a 4KiB boundary, not above 0xfffffffffffff000;
    virtual_start: u64,
    number_of_pages: u64,
    // describe bit mask of the capabilities of the memory region
    attribute: u64,
}

#[repr(C)]
struct EfiBootServices {
    header: EfiTableHeader,
    // Raises the task priority level
    _raise_tpl: usize,
    // Restores/Lowers the task priory level
    _restore_tpl: usize,
    _allocate_pages: usize,
    _free_pages: usize,
    get_memory_map: unsafe fn(
        memory_map_size: &mut usize,
        memory_map: *mut u8,
        map_key: &mut usize,
        descriptor_size: &mut usize,
        descriptor_version: &mut u32,
    ) -> EfiStatus,
    _allocale_pool: usize,
    _free_pool: usize,
    _create_event: usize,
    _set_timer: usize,
    _wait_for_event: usize,
    _signal_event: usize,
    _close_event: usize,
    _check_event: usize,
    _install_protocol_interface: usize,
    _reinstall_protocol_interface: usize,
    _uninstall_protocol_interface: usize,
    _handle_protocol: usize,
    _reserved: usize,
    _register_protocol_notify: usize,
    _locate_handle: usize,
    _locate_device_path: usize,
    _install_configuration_table: usize,
    _load_image: usize,
    _start_image: usize,
    _exit: usize,
    _unload_image: usize,
    exit_boot_services: unsafe fn(image_handle: EfiHandle, map_key: usize) -> EfiStatus,
}

#[repr(C)]
struct EfiSimpleTextInputProtocol {
    reset: unsafe fn(
        this: *const EfiSimpleTextInputProtocol,
        extended_verification: bool,
    ) -> EfiStatus,
    read_keystroke:
        unsafe fn(this: *const EfiSimpleTextInputProtocol, key: *mut EfiInputKey) -> EfiStatus,
    _wait_for_key: usize,
}

#[repr(C)]
struct EfiSimpleTextOutputProtocol {
    reset: unsafe fn(
        this: *const EfiSimpleTextOutputProtocol,
        extended_verification: bool,
    ) -> EfiStatus,

    // Writes a string to the output device
    output_string:
        unsafe fn(this: *const EfiSimpleTextOutputProtocol, string: *const u16) -> EfiStatus,
    // Verifies that all characters in a string can beoutput to the target
    // device.
    test_string:
        unsafe fn(this: *const EfiSimpleTextOutputProtocol, string: *const u16) -> EfiStatus,
    _query_mode: usize,
    _set_mode: usize,
    _set_attribute: usize,
    _clean_screen: usize,
    _set_cursor_position: usize,
    _enable_cursor: usize,
    _mode: usize,
}

/// Contains pointers to the runtime and boot service tables
#[repr(C)]
pub struct EfiSystemTable {
    header: EfiTableHeader,
    firmware_vendor: *const u16,
    firmware_revision: u32,
    console_in_handle: EfiHandle,
    console_in: *const EfiSimpleTextInputProtocol,
    console_out_handle: u32,
    console_out: *const EfiSimpleTextOutputProtocol,
    console_error_handle: u32,
    console_error: *const EfiSimpleTextOutputProtocol,
    _runtime_services: usize,
    boot_services: *const EfiBootServices,
}
