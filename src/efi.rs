use core::{
    mem::size_of,
    sync::atomic::{AtomicPtr, Ordering},
    usize,
};

use crate::mm::rangeset::{self, Range,RangeSet};


/// The maximum number of memory regions that we can save from EFI
const NUM_MEMORY_REGIONS: usize = 64;

/// A 'Result' type wrapping an EFI error.
type Result<T> = core::result::Result<T, Error>;

/// Errors from EFI calls.
#[derive(Debug)]
pub enum Error {
    /// The EFI System Table has not been registered.
    NotRegistered,

    /// EFI did not reort a valid ACPI table.
    AcpiTableNotFound,

    /// EFI system table was not found.
    EfiSystemTableNotFound,

    /// We failed to get the memory map from EFI.
    MemoryMap(EfiStatus),

    /// The memory map returned from EFI was did not fit within the bounds
    /// that it was reported.
    MemoryMapOutOfBounds,

    /// We failed exiting EFI boot services.
    ExitBootServices(EfiStatus),

    /// An integer overflow occured when processing EFI memory map data.
    MemoryMapIntegerOverflow,

    /// The EFI memory map had more entries than our fixed size array allows.
    MemoryMapOutOfEntries,

    /// An error occured when trying to construct the memory map `RangeSet`.
    MemoryRangeSet(rangeset::Error),
}

static EFI_SYSTEM_TABLE: AtomicPtr<EfiSystemTable> = AtomicPtr::new(core::ptr::null_mut());

/// A strongly typed EFI system table which will disallow the copying
/// of the raw pointer.
#[repr(transparent)]
pub struct EfiSystemTablePtr(*mut EfiSystemTable);

impl EfiSystemTablePtr {
    /// Register this system table into a global so it can be used for prints
    /// which do not take a self, or a pointer as an argument and thus this
    /// must be able to be found on a pointer.
    pub unsafe fn register(self) {
        EFI_SYSTEM_TABLE
        .compare_exchange(
            core::ptr::null_mut(),
            self.0,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .unwrap();
    }
}

pub fn output_string(string: &str) -> Result<()> {
    let system_table = EFI_SYSTEM_TABLE.load(Ordering::SeqCst);

    if system_table.is_null() {
        return Err(Error::EfiSystemTableNotFound);
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

        // If the temporary buffer could potentially be full on the next
        // iteration we flush it. We do -2 here because we need room for
        // the worst case which is a carriage return, newline, and null
        // terminator in the next iteration. We also need to do >= because 
        // we can potentially skip from 29 in use to 31 in use if the 30th 
        // character is a newline. 
        if in_use >= (tmp.len() - 2) {
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

    Ok(())
}

/// Get the base of the ACPI table RSD PTR (RSDP). If EFI did not report an ACPI
/// table, then we return `None`.
pub fn get_acpi_table() -> Result<usize> {

    /// ACPI 2.0 or newer tables should use EFI_ACPI_TABLE_GUID
    const EFI_ACPI_TABLE_GUID: EfiGuid = EfiGuid(
        0x8868e871,
        0xe4f1,
        0x11d3,
        [0xbc, 0x22, 0x00, 0x80, 0xc7, 0x3c, 0x88, 0x81],
    );
    
    /// ACPI 1.0 or newer tables should use EFI_ACPI_TABLE_GUID
    const ACPI_TABLE_GUID: EfiGuid = EfiGuid(
        0xeb9d2d30,
        0x2d88,
        0x11d3,
        [0x9a, 0x16, 0x00, 0x90, 0x27, 0x3f, 0xc1, 0x4d],
    );

    let system_table = EFI_SYSTEM_TABLE.load(Ordering::SeqCst);

    if system_table.is_null() {
        return Err(Error::AcpiTableNotFound);
    }

    // Convert system table into Rust reference
    let tables = unsafe {
        core::slice::from_raw_parts((*system_table).tables, (*system_table).number_of_tables)
    };

    // First look for the ACPI 2.0 table pointer, if we can't find it, then look
    // for the ACPI 1.0 table pointer
    tables
        .iter()
        .find_map(|EfiConfigurationTable { guid, table }| {
            (guid == &EFI_ACPI_TABLE_GUID).then_some(*table)
        })
        .or_else(|| {
            tables
                .iter()
                .find_map(|EfiConfigurationTable { guid, table }| {
                    (guid == &ACPI_TABLE_GUID).then_some(*table)
                })
        }).ok_or(Error::AcpiTableNotFound)
}

/// Holds a region of usable physical memory
#[derive(Debug, Clone, Copy)]
pub struct UsableMemory {
    
    /// Start address (inclusive) of the emory region
    pub start: u64,
    
    /// End address (inclusive) of the memory region
    pub end: u64,
}

pub fn get_memory_map(image_handle: EfiHandle) -> Result<RangeSet> {
    let system_table = EFI_SYSTEM_TABLE.load(Ordering::SeqCst);

    if system_table.is_null() {
        return Err(Error::NotRegistered);
    }

    let mut memory_map = [0u8; 4 * 1024];

    // The Rust memory map
    let mut usable_memory = RangeSet::new();

    unsafe {
        // Set up the initial arguments to get the `get_memory_map` EFI call.
        let mut size = core::mem::size_of_val(&memory_map);
        let mut key = 0;
        let mut mdesc_size = 0;
        let mut mdesc_version = 0;

        // Get the memory map.
        let ret= ((*(*system_table).boot_services).get_memory_map)(
            &mut size,
            memory_map.as_mut_ptr(),
            &mut key,
            &mut mdesc_size,
            &mut mdesc_version,
        ).into();
        
        // Check that the memory map is obtained.
        if ret != EfiStatus::Success {
            return Err(Error::MemoryMap(ret));
        }

        // Go through each memory map entry.
        for offset in (0..size).step_by(mdesc_size) {
            let entry = core::ptr::read_unaligned(
                memory_map.get(offset..)
                    .ok_or(Error::MemoryMapOutOfBounds)?
                    .get(..offset + size_of::<EfiMemoryDescriptor>())
                    .ok_or(Error::MemoryMapOutOfBounds)?
                    .as_ptr() as *const EfiMemoryDescriptor
            );

            let typ: EfiMemoryType = entry.typ.into();

            // Check if this memory is usable after boot services are exited
            if typ.avail_post_exit_boot_service() && entry.number_of_pages > 0 {
                // Get the number of bytes for this memory region
                let bytes = entry.number_of_pages.checked_mul(4096)
                    .ok_or(Error::MemoryMapIntegerOverflow)?;
                
                // Compute the end physical address of this region
                let end = entry.physical_start.checked_add(bytes - 1)
                    .ok_or(Error::MemoryMapIntegerOverflow)?;
                                 
                // Set the usable memory information
                usable_memory.insert(Range {
                    start: entry.physical_start,
                    end: end,
                }).map_err(|e| Error::MemoryRangeSet(e))?;
            }
           
        }

        // Exit Boot serices
        let ret = ((*(*system_table).boot_services).exit_boot_services)(
            image_handle,
            key
        ).into();

        if ret == EfiStatus::Success {
            return Err(Error::ExitBootServices(ret));
        }

        // Kill the EFI system table
        // EFI_SYSTEM_TABLE.store(core::ptr::null_mut(), Ordering::SeqCst);
    }
    
    Ok(usable_memory)
}

#[derive(Debug)]
#[repr(transparent)]
pub struct EfiHandle(usize);

#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub struct EfiStatusCode(usize);




/// EFI status codes
#[derive(Debug, PartialEq, Eq)]
pub enum EfiStatus {
    /// EFI Success
    Success,
    
    /// An EFI warning (top bit clear)
    Warning(EfiWarning),

    /// An EFI error (top bit set)
    Error(EfiError),
}

impl From<EfiStatusCode> for EfiStatus {
    fn from(val: EfiStatusCode) -> Self{
        
        // Sign extend the error code to make this not tied with a specific
        // bitness 
        let val = val.0 as i32 as i64 as u64;

        
        match val {
            0 => {
                Self::Success
            }
            0x0000000000000001..=0x7fffffffffffffff => {
                EfiStatus::Warning(match val & !(1<< 63) {
                    1 => EfiWarning::UnknownGlyph,
                    2 => EfiWarning::DeleteFailure,
                    3 => EfiWarning::WriteFailure,
                    4 => EfiWarning::BufferTooSmall,
                    5 => EfiWarning::StaleData,
                    6 => EfiWarning::FileSystem,
                    7 => EfiWarning::ResetRequired,
                    _ => EfiWarning::Unknown(val),
                })
            }
            0x8000000000000000..=0xffffffffffffffff => {
                EfiStatus::Error(match val & !(1<< 63) {
                    1 => EfiError::LoadError,
                    2 => EfiError::InvalidParameter,
                    3 => EfiError::Unsupported,
                    4 => EfiError::BadBufferSize,
                    5 => EfiError::BufferTooSmall,
                    6 => EfiError::NotReady,
                    7 => EfiError::DeviceError,
                    8 => EfiError::WriteProtected,
                    9 => EfiError::OutOfResources,
                    10 => EfiError::VolumeCorrupted,
                    11 => EfiError::VolumeFull,
                    12 => EfiError::NoMedia,
                    13 => EfiError::MediaChanged,
                    14 => EfiError::NotFound,
                    15 => EfiError::AccessDenied,
                    16 => EfiError::NoResponse,
                    17 => EfiError::NoMapping,
                    18 => EfiError::Timeout,
                    19 => EfiError::NotStarted,
                    20 => EfiError::AlreadyStarted,
                    21 => EfiError::Aborted,
                    22 => EfiError::IcmpError,
                    23 => EfiError::TftpError,
                    24 => EfiError::ProtocolError,
                    25 => EfiError::IncompatibleVersion,
                    26 => EfiError::SecurityViolation,
                    27 => EfiError::CrcError,
                    28 => EfiError::EndOfMedia,
                    31 => EfiError::EndOfFile,
                    32 => EfiError::InvalidLanguage,
                    33 => EfiError::CompromisedData,
                    34 => EfiError::IpAddressConflict,
                    35 => EfiError::HttpError,
                    _ => EfiError::Unknown(val),
                })
            }
        }
    }
}

/// EFI warning codes
#[derive(Debug, PartialEq, Eq)]
pub enum EfiWarning {
    /// The string contained one or more characters that the deice could not 
    /// render and were skipped
    UnknownGlyph,
    
    /// The handle was closed, but the file was not deleted
    DeleteFailure,
    
    /// The handle was closed, but the data to the file was not flushed properly
    WriteFailure,
    
    /// The resulting buffer was too small, and the data was truncated to the 
    /// buffer size
    BufferTooSmall,
    
    /// The data has not been updated within the timeframe set by the local 
    /// policy for this type of data
    StaleData,
    
    /// The resulting buffer contains UEFI-compliant file system 
    FileSystem,
    
    /// The operation will be processed accross a system reset
    ResetRequired,

    /// An Unknown warning
    Unknown(u64),
}

/// EFI error codes
#[derive(Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum EfiError {
    
    /// The image failed to load
    LoadError,
    
    /// A parameter was incorrect
    InvalidParameter,
    
    /// The operation is not supported
    Unsupported,
    
    /// The bufer was not the proper size for the request
    BadBufferSize,
    
    /// The buffer is not large enough to hold the requested data.
    BufferTooSmall,
    
    /// There is no data pending upon return
    NotReady,
    
    /// The physical device reported an error
    DeviceError,
    
    /// The device cannot be written to
    WriteProtected,
    
    /// A resource has run out
    OutOfResources,
    
    /// An inconsistency was detected on the file system causing the operation 
    /// to fail
    VolumeCorrupted,
    
    /// There is no more space on the file system
    VolumeFull,
    
    /// The device does not contain any medium to perform the operation
    NoMedia,
    
    /// The medium in the device has changed since the last access
    MediaChanged,
    
    /// The item was not found
    NotFound,
    
    /// Access was denied
    AccessDenied,
    
    /// The server was not found or did nor respond to the request
    NoResponse,
    
    /// A mapping to a device does not exist
    NoMapping,
    
    /// the timeout time expired
    Timeout,

    /// The protocol has not beem started
    NotStarted,
    
    /// The protocol has already been started
    AlreadyStarted,
    
    /// The operatuon was aborted
    Aborted,
    
    /// An ICMP error occured during the network operation
    IcmpError,
    
    /// A TFTP error occurred during the network operation
    TftpError,
    
    /// A protocol error occurred during the network operation
    ProtocolError,
    
    /// The function encountered an internal version that was incompatible with
    /// a version requested by the caller
    IncompatibleVersion,
    
    /// The function was not performed due to a security violation
    SecurityViolation,
    
    /// A CRC error was detected
    CrcError,
    
    /// Beginning or end of media reached
    EndOfMedia = 28,
    
    /// The end of the file was reached
    EndOfFile = 31,
    
    /// The language specified was invalid
    InvalidLanguage,
    
    /// The security status of the data is unknown or compromised and the data
    /// must be updated or replaced to restore a valid security status
    CompromisedData,
    
    /// The is an address conflict address allocation
    IpAddressConflict,
    
    /// An HTTP error occurred durin the network operation
    HttpError,

    /// An Unknown error
    Unknown(u64),
}

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
        matches!(
            self,
            EfiMemoryType::BootServiceCode
                | EfiMemoryType::BootServiceData
                | EfiMemoryType::ConventionalMemory
                | EfiMemoryType::PersistentMemory
        )
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
    ) -> EfiStatusCode,
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
    exit_boot_services: unsafe fn(image_handle: EfiHandle, map_key: usize) -> EfiStatusCode,
}

#[repr(C)]
struct EfiSimpleTextInputProtocol {
    reset: unsafe fn(
        this: *const EfiSimpleTextInputProtocol,
        extended_verification: bool,
    ) -> EfiStatusCode,
    read_keystroke:
        unsafe fn(this: *const EfiSimpleTextInputProtocol, key: *mut EfiInputKey) -> EfiStatusCode,
    _wait_for_key: usize,
}

#[repr(C)]
struct EfiSimpleTextOutputProtocol {
    reset: unsafe fn(
        this: *const EfiSimpleTextOutputProtocol,
        extended_verification: bool,
    ) -> EfiStatusCode,

    // Writes a string to the output device
    output_string:
        unsafe fn(this: *const EfiSimpleTextOutputProtocol, string: *const u16) -> EfiStatusCode,
    // Verifies that all characters in a string can beoutput to the target
    // device.
    test_string:
        unsafe fn(this: *const EfiSimpleTextOutputProtocol, string: *const u16) -> EfiStatusCode,
    _query_mode: usize,
    _set_mode: usize,
    _set_attribute: usize,
    _clean_screen: usize,
    _set_cursor_position: usize,
    _enable_cursor: usize,
    _mode: usize,
}

/// Provides access to UEFI Boot Services, UEFI Runtime Services, consoles,
/// firmware vendor information, and the system configuration tables.
#[repr(C)]
struct EfiSystemTable {
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

    number_of_tables: usize,
    tables: *const EfiConfigurationTable,
}

/// Contains a set of GUID/pointer pairs comprised of the
/// ConfigurationTable field in the EFI System Table.
#[derive(Debug)]
#[repr(C)]
struct EfiConfigurationTable {
    // The 128-bit GUID value that uniquely identifies the system configuration
    // table.
    guid: EfiGuid,
    // A pointer to the table associated with VendorGuid (`guid`). Type of the
    // memory that is used to store the table as well as whether this
    // pointer is a physical address or a virtual address during runtime
    // (whether or not a particular address reported in the table gets fixed
    // up when a call to SetVirtualAddressMap() is made) is
    // determined by the VendorGuid. Unless otherwise specified,
    // memory type of the table buffer is defined by the guidelines set
    // forth in the Calling Conventions section in Chapter 2. It is the
    // responsibility of the specification defining the VendorTable to
    // specify additional memory type requirements (if any) and whether
    // to convert the addresses reported in the table. Any required address
    // conversion is a responsibility of the driver that publishes
    // corresponding configuration table.
    table: usize,
}
/// 128-bit buffer containing a unique identifier value. Unless otherwise
/// specified, aligned on a 64-bit boundary.
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
struct EfiGuid(u32, u16, u16, [u8; 8]);
