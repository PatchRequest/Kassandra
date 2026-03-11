use std::arch::asm;

/// Walk the PEB's InMemoryOrderModuleList and count all loaded modules.
pub unsafe fn count_loaded_modules() -> usize {
    let peb: *const u8;
    asm!("mov {}, gs:[0x60]", out(reg) peb);

    // PEB->Ldr is at offset 0x18
    let ldr = *(peb.add(0x18) as *const *const u8);

    // PEB_LDR_DATA->InMemoryOrderModuleList is at offset 0x20
    let head = ldr.add(0x20);

    // First entry: follow Flink
    let mut current = *(head as *const *const u8);
    let mut count: usize = 0;

    // Walk until we loop back to the head
    while current != head {
        count += 1;
        current = *(current as *const *const u8);
    }

    count
}
