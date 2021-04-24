//! An very lightweight ACPI implementation for extracting basic information
//! about CPU topography and NUMA memory regions

use core::intrinsics::size_of;

use crate::efi;
use crate::mm::{self, PhysAddr};

/// A `Result` type that wraps and ACPI error
type Result<T> = core::result::Result<T, Error>;

/// Different types of ACPI tables, mainly used for error information.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableType {
    /// The root system description pointer (ACPI 1.0).
    Rsdp,

    /// The extended root systen description pointer (ACPI 2.0).
    RsdpExtended,

    /// Extended Systen Description Table
    Xsdt,

    /// Multiple APIC (Advanced Programmable Interrupt Controller) Description Table
    Madt,

    /// System Resource Affinity Table
    Srat,

    /// Unknown table type
    Unknown([u8; 4]),
}

impl From<[u8; 4]> for TableType {
    fn from(val: [u8; 4]) -> Self {
        match &val {
            b"XSDT" => Self::Xsdt,
            b"APIC" => Self::Madt,
            b"SRAT" => Self::Srat,
            _ => Self::Unknown(val),
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

    // The XSDT table size was not evenly divisible by the array element size
    XsdtBadEntries,

    // An integer overflow occurred
    IntegerOverflow,

}

/// Compute an ACPI checksum on physical memory
unsafe fn checksum(addr: PhysAddr, size: usize, typ: TableType) -> Result<()> {
    // Compute and validate the checksum
    let chk = (0..size as u64).try_fold(0u8, |acc, offset| {
        Ok(acc.wrapping_add(mm::read_phys::<u8>(PhysAddr(
            addr.0.checked_add(offset).ok_or(Error::IntegerOverflow)?,
        ))))
    })?;

    if chk == 0 {
        Ok(())
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
        checksum(addr, size_of::<Self>(), TableType::Rsdp)?;

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

/// Attempt to process `addr` as an ACPI table. Return an Error if it is not a
/// valid ACPI table. Returns (table header, table type, content address,
/// payload_size).
impl Table {
    // From an Addr check the validity of the Table.
    unsafe fn from_addr(addr: PhysAddr) -> Result<(Self, TableType, PhysAddr, usize)> {
        // Read the table.
        let table = mm::read_phys::<Self>(addr);

        // Get the type of this table.
        let typ = TableType::from(table.signature);

        // Validate the checksum.
        checksum(addr, table.length as usize, typ)?;

        // Make sure the table length is sane.
        let header_size = size_of::<Self>();

        let payload_addr = PhysAddr(
            addr.0
                .checked_add(header_size as u64)
                .ok_or(Error::IntegerOverflow)?,
        );
        let payload_size = (table.length as usize)
            .checked_sub(header_size)
            .ok_or(Error::LengthMismatch(typ))?;

        Ok((table, typ, payload_addr, payload_size))
    }
}

struct Madt {}

impl Madt {
    /// Process the payload of an MADT based on a physical address and a size
    unsafe fn from_addr(addr: PhysAddr, size: usize) -> Result<Self> {
        /// The error type when the MADT is truncated
        const E: Error = Error::LengthMismatch(TableType::Madt);
        
        // Create a slice to the physical memory
        let mut slice = mm::PhysSlice::new(addr, size);

        // Read the local APIC physical address
        let local_apic_addr = slice.consume::<u32>().map_err(|_| E)?;

        // Get the APIC flags
        let flags = slice.consume::<u32>().map_err(|_| E)?;

        
        // Handle interrup controller structures
        while slice.len() > 0 {
            // Read the interrupt controller header
            let typ = slice.consume::<u8>().map_err(|_| E)?;
            let len = slice.consume::<u8>().map_err(|_| E)?
                .checked_sub(2).ok_or(E)?;
            
            match typ {
                
                0 => {
                    #[repr(C, packed)]
                    struct LocalApic {
                        
                        /// The OS associates this local apic structure with a
                        /// processor object in the namespace when the _UID
                        /// child object of the processor's device object (or 
                        /// ProcessorId listed in the processor declaration 
                        /// operator) evaluates to a numeric value that matches
                        /// the numeric value in the field 
                        acpi_processor_uid: u8,
                        
                        /// The processor's local APIC ID.
                        apic_id: u8,
                        
                        /// Local APIC flags
                        ///
                        /// Bit 0: Enabled (set if ready for use)
                        /// Bit 1: Online capable (RAZ is enabled, indicates if 
                        /// the APIC can be enabled at runtime)
                        flags: u32,
                    }

                    // Ensure the data is the correct size
                    if len as usize != size_of::<LocalApic>() {
                        return Err(E);
                    }

                    let apic = slice.consume::<LocalApic>().map_err(|_| E);
                }
                
                9 => {
                    // Processor Local x2APIC structure
                    #[repr(C, packed)]
                    struct LocalX2apic{
                        /// Reserved, must be zero
                        reserved: u16,
                        
                        /// The processor's local x2APIC ID 
                        x2apic_id: u32,
                        
                        /// Same as local APIC flags 
                        flags: u32,
                        
                        /// OSPM associates the X2APIC Structure with a processor
                        /// object declared in the namespace using the Device
                        /// statement, when the _UID child object of the 
                        /// processor device evaluates to a numeric value, by
                        /// matching the numeric value with this field 
                        acpi_processor_uid: u32,
                    }

                    // Ensure the data is the correct size
                    if len as usize != size_of::<LocalX2apic>() {
                        return Err(E);
                    }

                    let x2_apic = slice.consume::<LocalX2apic>().map_err(|_| E);

                }
                _ => {
                    // Unknown type, discard the data
                    slice.discard(len as usize).map_err(|_| E)?;
                }
            }
            
        }

        panic!();
    }
}

/// Initialize the ACPI subsystem.
pub unsafe fn init() -> Result<()> {
    // Get the ACPI table base from EFI.
    let rsdp_addr = efi::get_acpi_table().ok_or(Error::RsdpNotFound)?;

    // Validate and get the RSDP.
    let rsdp = RsdpExtended::from_addr(PhysAddr(rsdp_addr as u64))?;

    // Get the XSDT
    let (_, typ, xsdt, length) = 
        Table::from_addr(PhysAddr(rsdp.xsdt_addr))?;
    if typ != TableType::Xsdt {
        return Err(Error::SignatureMismatch(typ));
    }

    // Make sure the XSDT size is modulo a 64-bit address size
    if length % size_of::<u64>() != 0 {
        return Err(Error::XsdtBadEntries);
    }
    // Get the number of entries in the XSDT
    let entries = length / size_of::<u64>();

    print!("XSDT entries {}\n", entries);

    // Go through each table in the XSDT
    for idx in 0..entries {
        // Get the physical address of the XSDT entry
        let entry_addr = idx
            .checked_mul(size_of::<u64>())
            .and_then(|x| x.checked_add(xsdt.0 as usize))
            .ok_or(Error::IntegerOverflow)?;

        // Get the table address by reading the XSDT entry.
        // It has been observed in OVMF that these addresses indeed can be unaligned.
        let table_addr = mm::read_phys_unaligned::<u64>(PhysAddr(entry_addr as u64));

        // Parse and validate the table header
        let (_, typ, data, length) = Table::from_addr(PhysAddr(table_addr))?;

        match typ {
            TableType::Madt => {
                Madt::from_addr(data, length)?;
            }

            // Unknown
            _ => {}
        }
    }
    Ok(())
}
