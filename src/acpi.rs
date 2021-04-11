//! An very lightweight ACPI implementation for extracting basic information
//! about CPU topography and NUMA memory regions

use core::intrinsics::size_of;

use crate::efi;
use crate::mm::{self, PhysAddr};

/// A `Result` type that wraps and ACPI error
type Result<T> = core::result::Result<T, Error>;

/// Different types of ACPI tables, mainly used for error information.
#[derive(Clone, Copy, Debug)]
pub enum TableType {
    /// The root system description pointer (ACPI 1.0).
    Rsdp,
    /// The extended root systen description pointer (ACPI 2.0).
    RsdpExtended,
    /// Extended Systen Description Table
    Xsdt,


    /// Unknown table type
    Unknown([u8;4]),
}

impl From<[u8;4]> for TableType {
    fn from(val: [u8;4]) -> Self {
        match &val {
            b"XSDT" => Self::Xsdt,
            
            _       => Self::Unknown(val),
        }
    }

}

/// Errors from ACPI table parsing.
#[derive(Debug)]
pub enum Error {
    /// The ACPI table address was not reported by UEFI and thus we were unable
    /// tofind the RSDP.
    RsdpNotFound,

    /// An ACPI table had an invalid checksum.
    ChecksumMismatch(TableType),

    /// An ACPI table did not match the correct signature.
    SignatureMismatch(TableType),

    /// The ACPI table did not match the expected length
    LengthMismatch(TableType),

    /// The Extended RSDP was attempted to be accessed however the ACPI version
    /// for this system was too old to support it. ACPI 2.0 is required.
    RevisionTooOld,
}


/// Compute an ACPI checksum on physical memory 
unsafe fn checksum(addr: PhysAddr, size: usize, typ: TableType) -> Result<()> {

    // Compute and validate the checksum
    let chk = (0 .. size as u64).fold(0u8, |acc, offset| {
        acc.wrapping_add(mm::read_phys::<u8>(PhysAddr(addr.0 + offset)))
    });

    if chk == 0 {
        Ok (())
    } else {
        Err(Error::ChecksumMismatch(typ))
    }

    
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

    /// 32 bit physical address of the RSDT.
    rsdt_addr: u32,
}

impl Rsdp {
    /// Load an Rsdp structure from `addr`.
    unsafe fn from_addr(addr: PhysAddr) -> Result<Self> {
        
        // Validate the checksum
        checksum(addr,size_of::<Self>(), TableType::Rsdp)?;
        
        // Get the RSDP table
        let rsdp = mm::read_phys::<Self>(addr);

        // Check the signature.
        if &rsdp.signature != b"RSD PTR " {
            return Err(Error::SignatureMismatch(TableType::Rsdp));
        }

        // Return the RSDP.
        Ok(rsdp)
    }
}

/// In-memory representation of an Extended RSDP ACPI structure.
#[repr(C, packed)]
struct RsdpExtended {
    /// Base level RSDP table for ACPI 1.0.
    base: Rsdp,

    /// The length of the table, in bytes, including the header, starting
    /// from offset 0. This field is used to record the size of the entire
    /// table. This field is not available in the ACPI version 1.0 RSDP
    /// structure.
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
        // First, start by reading the RSDP. This is the ACPI 1.0 structure and
        // thus is a subset and backwards compatible with all future revisions.
        let rsdp = Rsdp::from_addr(addr)?;
        
        // The extended RSDP required ACPI 2.0.
        if rsdp.revision < 2 {
            return Err(Error::RevisionTooOld);
        }

        // Validate the checksum
        checksum(addr, size_of::<Self>(), TableType::RsdpExtended)?;

        // Get the extended RSDP table
        let rsdp = mm::read_phys::<Self>(addr);

        // Check the size
        if rsdp.length as usize != size_of::<Self>() {
            return Err(Error::LengthMismatch(TableType::RsdpExtended));
        }

        // Return the RSDP
        Ok(rsdp)
    }
}

/// In-memory representation of an ACPI table header.
#[repr(C, packed)]
struct Table {
    
    /// The ASCII string representation of the table identifier.
    signature: [u8; 4],
    
    /// The length of the table, in bytes, including the header, starting from
    /// offset 0. This field is used to record the size of the entire table.
    length: u32,
    
    /// The revision of the structure corresponding to the signature field
    /// for this table.
    revision: u8,
    
    /// The entire table, including the checksum field, must add to zero to be 
    /// considered valid.
    checksum: u8,
    
    /// An OEM-supplied string that identifies the OEM.
    oemid: [u8; 6],
    
    /// An OEM-supplied string that the OEM uses to identify the particular 
    /// data table. This field is particularly useful when defining a 
    /// definition block to distinguish definition block functions. 
    /// The OEM assigns each dissimilar table a new OEM Table ID.
    oem_table_id: u64,
    
    /// An OEM-supplied revision number. Larger numbers are assumed to be 
    /// newer revisions.
    oem_revision: u32,
    
    /// Vendor ID of utility that created the table. For tables containing 
    /// Definition Blocks, this is the ID for the ASL Compiler.
    creator_id: u32,
    
    /// Revision of utility that created the table. For tables containing 
    /// Definition Blocks, this is the revision for the ASL Compiler.
    creator_revision: u32,
}

impl Table {
    // From an Addr check the validity of the Table
    unsafe fn from_addr(addr: PhysAddr) -> Result<(Self, TableType)> {
        
        // Read the table
        let table = mm::read_phys::<Self>(addr);
        
        // Get the type of this table
        let typ = TableType::from(table.signature);
        
        // Validate the checksum
        checksum(addr, table.length as usize, typ)?;

        Ok((table, typ))
    }
}

/// Initialize the ACPI subsystem.
pub unsafe fn init() -> Result<()> {
    // Get the ACPI table base from EFI.
    let rsdp_addr = efi::get_acpi_table().ok_or(Error::RsdpNotFound)?;

    // Validate and get the RSDP.
    let rsdp = RsdpExtended::from_addr(PhysAddr(rsdp_addr as u64))?;
    
    // Get the XSDT
    let _xsdt = Table::from_addr(PhysAddr(rsdp.xsdt_addr))?;
    
    Ok(())
}
