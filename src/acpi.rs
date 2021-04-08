//! An very lightweight ACPI implementation for extracting basic information
//! about CPU topography and NUMA memory regions

use core::mem::size_of;

use crate::mm::{self, PhysAddr};
use crate::efi;

/// Root System Description Pointer (RSDP) structure.
#[derive(Clone, Copy)]
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
    reserved: [u8;3],

}

/// In-memory representation of an Extended RSDP ACPI structure
#[derive(Clone, Copy)]
#[repr(C, packed)]
struct RsdpExtended {
    descriptor:        Rsdp,
    length:            u32,
    xsdt_addr:         u64,
    extended_checksum: u8,
    reserved:          [u8; 3],
}

/// In-memory representation of an ACPI table header
#[derive(Clone, Copy)]
#[repr(C, packed)]
struct Header {
    signature:        [u8; 4],
    length:           u32,
    revision:         u8,
    checksum:         u8,
    oemid:            [u8; 6],
    oem_table_id:     u64,
    oem_revision:     u32,
    creator_id:       u32,
    creator_revision: u32,
}

/// Parse a standard ACPI table header. This will parse out the header,
/// validate the checksum and length, and return a physical address and size
/// of the payload following the header.
unsafe fn parse_header(addr: PhysAddr) -> (Header, PhysAddr, usize) {
    // Read the header
    let head = mm::read_phys::<Header>(addr);

    // Get the number of bytes for the table
    let payload_len = head.length
        .checked_sub(size_of::<Header>() as u32)
        .expect("Integer underflow on table length");

    // Check the checksum for the table
    let sum = (addr.0..addr.0 + head.length as u64)
        .fold(0u8, |acc, paddr| {
            acc.wrapping_add(mm::read_phys(PhysAddr(paddr as u64)))
        });
    assert!(sum == 0, "Table checksum invalid {:?}",
            core::str::from_utf8(&head.signature));

    // Return the parsed header
    (head, PhysAddr(addr.0 + size_of::<Header>() as u64), payload_len as usize)
}

/// Initialize the ACPI subsystem. Mainly looking for APICs and memory maps.
/// Brings up all cores on the system
pub unsafe fn init() {
    
    // Get the ACPI table base from EFI 
    let rsdp_addr = efi::get_acpi_table()
        .expect("Failed to get RSDP address from EFI");
    let rsdp = core::ptr::read_unaligned(rsdp_addr as *const Rsdp);
    
    // Check information about the RSDP
    assert!(&rsdp.signature == b"RSD PTR ", "RSDT signature mismatch.");
    assert!(rsdp.length as usize >= size_of::<Rsdp>(), "RSDP size invalid." );
    assert!(rsdp.revision >= 1, "Minimum ACPI 2.0 version required.");

    // Parse out the XSDT
    let (xsdt, xsdt_payload, xsdt_size) =
        parse_header(PhysAddr(rsdp.xsdt_addr));

    // Check the signature and alignment of the structure
    assert!(&xsdt.signature == b"XSDT", "XSDT alignment mismatch.");
    assert!((xsdt_size % size_of::<u64>()) == 0,
    "Invalid table size for XSDT");
    
    let xsdt_entries = xsdt_size / size_of::<u64>();
    

    // Go through each table described by the XSDT
    for entry in 0..xsdt_entries {
        // Get the physical address of the XSDP table entry
        let entry_paddr = xsdt_payload.0 as usize + entry * size_of::<u64>();

        // Get the pointer to the table
        let table_ptr: u32 = mm::read_phys(PhysAddr(entry_paddr as u64));

        // Get the signature for the table
        let signature: [u8; 4] = mm::read_phys(PhysAddr(table_ptr as u64));

        if &signature == b"APIC" {
            // Parse the MADT
            parse_madt(PhysAddr(table_ptr as u64));
        } else if &signature == b"SRAT" {
            // Parse the SRAT
            parse_srat(PhysAddr(table_ptr as u64)); 
        }
    }
}

/// Parse the MADT out of the ACPI tables
/// Returns a vector of all usable APIC IDs
unsafe fn parse_madt(ptr: PhysAddr) {
    // Parse the MADT header
    let (_header, payload, size) = parse_header(ptr);

    // Skip the local interrupt controller address and the flags to get the
    // physical address of the ICS
    let mut ics = PhysAddr(payload.0 + 4 + 4);
    let end = payload.0 + size as u64;

    loop {
        /// Processor is ready for use
        const APIC_ENABLED: u32 = 1 << 0;

        /// Processor may be enabled at runtime (IFF ENABLED is zero),
        /// otherwise this bit is RAZ
        const APIC_ONLINE_CAPABLE: u32 = 1 << 1;

        // Make sure there's room for the type and the length
        if ics.0 + 2 > end { break; }

        // Parse out the type and the length of the ICS entry
        let typ: u8 = mm::read_phys(PhysAddr(ics.0 + 0));
        let len: u8 = mm::read_phys(PhysAddr(ics.0 + 1));

        // Make sure there's room for this structure
        if ics.0 + len as u64 > end { break; }
        assert!(len >= 2, "Bad length for MADT ICS entry");

        match typ {
            0 => {
                // LAPIC entry
                assert!(len == 8, "Invalid LAPIC ICS entry");

                // Read the APIC ID
                let apic_id: u8  = mm::read_phys(PhysAddr(ics.0 + 3));
                let flags:   u32 = mm::read_phys(PhysAddr(ics.0 + 4));

                // If the processor is enabled, or can be enabled, log it as
                // a valid APIC
                if (flags & APIC_ENABLED) != 0 ||
                        (flags & APIC_ONLINE_CAPABLE) != 0 {
                    // apics.push(apic_id as u32);
                }
            }
            9 => {
                // x2apic entry
                assert!(len == 16, "Invalid x2apic ICS entry");

                // Read the APIC ID
                let apic_id: u32 = mm::read_phys(PhysAddr(ics.0 + 4));
                let flags:   u32 = mm::read_phys(PhysAddr(ics.0 + 8));

                // If the processor is enabled, or can be enabled, log it as
                // a valid APIC
                if (flags & APIC_ENABLED) != 0 ||
                        (flags & APIC_ONLINE_CAPABLE) != 0 {
                    // apics.push(apic_id);
                }
            }
            _ => {
                // Don't really care for now
            }
        }

        // Go to the next ICS entry
        ics = PhysAddr(ics.0 + len as u64);
    }

}

/// Parse the SRAT out of the ACPI tables
/// Returns a tuple of (apic -> domain, memory domain -> phys_ranges)
unsafe fn parse_srat(ptr: PhysAddr) {
    // Parse the SRAT header
    let (_header, payload, size) = parse_header(ptr);

    // Skip the 12 reserved bytes to get to the SRA structure
    let mut sra = PhysAddr(payload.0 + 4 + 8);
    let end = payload.0 + size as u64;

    loop {
        /// The entry is enabled and present. Some BIOSes may staticially
        /// allocate these table regions, thus the flags indicate whether the
        /// entry is actually present or not.
        const FLAGS_ENABLED: u32 = 1 << 0;

        // Make sure there's room for the type and the length
        if sra.0 + 2 > end { break; }

        // Parse out the type and the length of the ICS entry
        let typ: u8 = mm::read_phys(PhysAddr(sra.0 + 0));
        let len: u8 = mm::read_phys(PhysAddr(sra.0 + 1));

        // Make sure there's room for this structure
        if sra.0 + len as u64 > end { break; }
        assert!(len >= 2, "Bad length for SRAT SRA entry");

        match typ {
            0 => {
                // Local APIC
                assert!(len == 16, "Invalid APIC SRA entry");

                // Extract the fields we care about
                let domain_low:  u8      = mm::read_phys(PhysAddr(sra.0 + 2));
                let domain_high: [u8; 3] = mm::read_phys(PhysAddr(sra.0 + 9));
                let apic_id:     u8      = mm::read_phys(PhysAddr(sra.0 + 3));
                let flags:       u32     = mm::read_phys(PhysAddr(sra.0 + 4));

                // Parse the domain low and high parts into an actual `u32`
                let domain = [domain_low,
                    domain_high[0], domain_high[1], domain_high[2]];
                let domain = u32::from_le_bytes(domain);

                // Log the affinity record
                if (flags & FLAGS_ENABLED) != 0 {

                }
            }
            1 => {
                // Memory affinity
                assert!(len == 40, "Invalid memory affinity SRA entry");

                // Extract the fields we care about
                let domain: u32      = mm::read_phys(PhysAddr(sra.0 +  2));
                let base:   PhysAddr = mm::read_phys(PhysAddr(sra.0 +  8));
                let size:   u64      = mm::read_phys(PhysAddr(sra.0 + 16));
                let flags:  u32      = mm::read_phys(PhysAddr(sra.0 + 28));

                // Only process ranges with a non-zero size (observed on
                // polar and grizzly that some ranges were 0 size)
                if size > 0 {
                    // Log the affinity record
                    if (flags & FLAGS_ENABLED) != 0 {
                    }
                }
            }
            2 => {
                // Local x2apic
                assert!(len == 24, "Invalid x2apic SRA entry");

                // Extract the fields we care about
                let domain:  u32 = mm::read_phys(PhysAddr(sra.0 +  4));
                let apic_id: u32 = mm::read_phys(PhysAddr(sra.0 +  8));
                let flags:   u32 = mm::read_phys(PhysAddr(sra.0 + 12));

                // Log the affinity record
                if (flags & FLAGS_ENABLED) != 0 {
                }
            }
            _ => {
            }
        }
        
        // Go to the next ICS entry
        sra = PhysAddr(sra.0 + len as u64);
    }
}