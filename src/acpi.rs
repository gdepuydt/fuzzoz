//! An very lightweight ACPI implementation for extracting basic information
//! about CPU topography and NUMA memory regions

use crate::efi;
use crate::mm::{self, PhysAddr};

/// A `Result` type that wraps and ACPI error
type Result<T> = core::result::Result<T, Error>;

/// Errors from ACPI table parsing
pub enum Error {
    /// The ACPI table address was not reported by UEFI and thus we were unable
    /// tofind the RSDP
    RsdpNotFound,
}

/// Root System Description Pointer (RSDP) structure for ACPI 1.0.
#[repr(C, packed)]
struct Rsdp {
    /// "RSD PTR "
    signature: [u8; 8],

    /// his is the checksum of the fields defined in the ACPI 1.0 specification.
    /// This includes only the first 20 bytes of this table, bytes 0 to 19,
    /// including the checksum field. These bytes must sum to zero.
    checksum: u8,

    /// An OEM-supplied string that identifies the OEM.
    oem_id: [u8; 6],

    /// The revision of this structure. Larger revision numbers are backward
    /// compatible to lower revision numbers. The ACPI version
    /// 1.0 revision number of this table is zero. The ACPI version 1.0
    /// RSDP Structure only includes the first 20 bytes of this table, bytes
    /// 0 to 19. It does not include the Length field and beyond.
    /// The current value for this field is 2.
    revision: u8,

    /// 32 bit physical address of the RSDT
    rsdt_addr: u32,
}

impl Rsdp {
    /// Load an Rsdp structure from `addr`
    unsafe fn from_addr(paddr: PhysAddr) -> Result<Self> {
        // Read the base RSDP structure
        let rsdp = mm::read_phys::<Rsdp>(paddr);
        todo!()
    }
}

/// In-memory representation of an Extended RSDP ACPI structure
#[repr(C, packed)]
struct RsdpExtended {
    /// Base level RSDP table for ACPI 1.0
    base: Rsdp,

    /// The length of the table, in bytes, including the header, starting
    /// from offset 0. This field is used to record the size of the entire
    /// table. This field is not available in the ACPI version 1.0 RSDP
    /// structure
    length: u32,

    /// 64 bit physical address of the XSDT.
    xsdt_addr: u64,

    /// This is a checksum of the entire table, including both checksum fields.
    extended_checksum: u8,

    /// Reserved field.
    reserved: [u8; 3],
}

impl RsdpExtended {
    unsafe fn from_addr(addr: PhysAddr) -> Result<Self> {
        // Read the base RSDP structure from physical memory
        let rdsp = Rsdp::from_addr(addr)?;
        todo!()
    }
}

/// In-memory representation of an ACPI table header
#[derive(Clone, Copy)]
#[repr(C, packed)]
struct Header {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oemid: [u8; 6],
    oem_table_id: u64,
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

/// Initialize the ACPI subsystem. Mainly looking for APICs and memory maps.
/// Brings up all cores on the system
pub unsafe fn init() -> Result<()> {
    // Get the ACPI table base from EFI
    let rsdp_addr = efi::get_acpi_table().ok_or(Error::RsdpNotFound)?;

    // Validate and get the RSDP
    let rsdp = RsdpExtended::from_addr(PhysAddr(rsdp_addr as u64))?;
    Ok(())
}
