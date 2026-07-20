#![allow(non_snake_case, non_camel_case_types)]

use crate::mem_wipe;
use std::ffi::c_void;
use std::ptr;

// --- PE structs ---

#[repr(C, packed(2))]
struct DosHeader {
    e_magic: u16,
    _pad: [u16; 29],
    e_lfanew: i32,
}

#[repr(C)]
struct FileHeader {
    Machine: u16,
    NumberOfSections: u16,
    _ts: u32,
    _sym: u32,
    _nsym: u32,
    SizeOfOptionalHeader: u16,
    _chars: u16,
}

#[repr(C, packed(4))]
struct OptionalHeader64 {
    Magic: u16,
    _linker: [u8; 2],
    _sizes: [u32; 3],
    AddressOfEntryPoint: u32,
    _basecode: u32,
    ImageBase: u64,
    SectionAlignment: u32,
    FileAlignment: u32,
    _versions: [u16; 6],
    _win32ver: u32,
    SizeOfImage: u32,
    SizeOfHeaders: u32,
    _checksum: u32,
    _subsystem: u16,
    _dllchars: u16,
    _stackheap: [u64; 4],
    _loaderflags: u32,
    NumberOfRvaAndSizes: u32,
    DataDirectory: [DataDirectory; 16],
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct DataDirectory {
    VirtualAddress: u32,
    Size: u32,
}

#[repr(C)]
struct SectionHeader {
    Name: [u8; 8],
    VirtualSize: u32,
    VirtualAddress: u32,
    SizeOfRawData: u32,
    PointerToRawData: u32,
    _relocs: u32,
    _lines: u32,
    _nrelocs: u16,
    _nlines: u16,
    Characteristics: u32,
}

#[repr(C)]
struct BaseRelocation {
    VirtualAddress: u32,
    SizeOfBlock: u32,
}

#[repr(C)]
struct ImportDescriptor {
    OriginalFirstThunk: u32,
    _ts: u32,
    _forwarder: u32,
    Name: u32,
    FirstThunk: u32,
}

#[repr(C)]
struct RuntimeFunction {
    _begin: u32,
    _end: u32,
    _unwind: u32,
}

// --- WinAPI FFI (import resolution + exception tables only) ---

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn RtlAddFunctionTable(table: *const RuntimeFunction, count: u32, base: u64) -> u8;
    fn RtlDeleteFunctionTable(table: *const RuntimeFunction) -> u8;
}

const PAGE_READWRITE: u32 = 0x04;
const PAGE_EXECUTE_READWRITE: u32 = 0x40;

type DllMainFn = unsafe extern "system" fn(hinstance: *mut c_void, reason: u32, reserved: *mut c_void) -> i32;

// --- Public API ---

pub struct MappedModule {
    base: *mut u8,
    size: usize,
    entry_point: Option<DllMainFn>,
    exception_table: *const RuntimeFunction,
}

impl MappedModule {
    pub unsafe fn get_export(&self, name: &str) -> Option<*const ()> {
        let dos = &*(self.base as *const DosHeader);
        let nt = self.base.add(dos.e_lfanew as usize);
        let opt = nt.add(4 + 20) as *const OptionalHeader64;

        let export_dir = (*opt).DataDirectory[0];
        if export_dir.VirtualAddress == 0 || export_dir.Size == 0 {
            return None;
        }

        let exp = self.base.add(export_dir.VirtualAddress as usize);
        let num_names = (exp.add(24) as *const u32).read_unaligned();
        let funcs_rva = (exp.add(28) as *const u32).read_unaligned();
        let names_rva = (exp.add(32) as *const u32).read_unaligned();
        let ords_rva = (exp.add(36) as *const u32).read_unaligned();

        let names = self.base.add(names_rva as usize) as *const u32;
        let ords = self.base.add(ords_rva as usize) as *const u16;
        let funcs = self.base.add(funcs_rva as usize) as *const u32;

        for i in 0..num_names as usize {
            let name_rva = names.add(i).read_unaligned();
            let export_name = self.base.add(name_rva as usize);

            let mut matches = true;
            for (j, &b) in name.as_bytes().iter().enumerate() {
                if *export_name.add(j) != b {
                    matches = false;
                    break;
                }
            }
            if matches && *export_name.add(name.len()) == 0 {
                let ordinal = ords.add(i).read_unaligned() as usize;
                let func_rva = funcs.add(ordinal).read_unaligned();
                return Some(self.base.add(func_rva as usize) as *const ());
            }
        }
        None
    }

    pub unsafe fn unload(self) {
        if let Some(entry) = self.entry_point {
            entry(self.base as *mut c_void, 0, ptr::null_mut());
        }
        if !self.exception_table.is_null() {
            RtlDeleteFunctionTable(self.exception_table);
        }
        mem_wipe::wipe_and_free(self.base, self.size);
    }
}

pub unsafe fn load(dll_bytes: &[u8]) -> Result<MappedModule, &'static str> {
    if dll_bytes.len() < 0x40 {
        return Err("too small");
    }
    if dll_bytes[0] != b'M' || dll_bytes[1] != b'Z' {
        return Err("bad DOS sig");
    }

    let e_lfanew = u32::from_le_bytes([dll_bytes[60], dll_bytes[61], dll_bytes[62], dll_bytes[63]]) as usize;
    if dll_bytes.len() < e_lfanew + 4 + 20 + 112 {
        return Err("headers truncated");
    }

    let pe_sig = u32::from_le_bytes([dll_bytes[e_lfanew], dll_bytes[e_lfanew+1], dll_bytes[e_lfanew+2], dll_bytes[e_lfanew+3]]);
    if pe_sig != 0x4550 {
        return Err("bad PE sig");
    }

    let file_hdr = e_lfanew + 4;
    let num_sections = u16::from_le_bytes([dll_bytes[file_hdr+2], dll_bytes[file_hdr+3]]) as usize;
    let size_of_opt_hdr = u16::from_le_bytes([dll_bytes[file_hdr+16], dll_bytes[file_hdr+17]]) as usize;
    let opt_hdr = file_hdr + 20;

    let size_of_image = u32::from_le_bytes([
        dll_bytes[opt_hdr+56], dll_bytes[opt_hdr+57], dll_bytes[opt_hdr+58], dll_bytes[opt_hdr+59]
    ]) as usize;
    let size_of_headers = u32::from_le_bytes([
        dll_bytes[opt_hdr+60], dll_bytes[opt_hdr+61], dll_bytes[opt_hdr+62], dll_bytes[opt_hdr+63]
    ]) as usize;
    let image_base = u64::from_le_bytes([
        dll_bytes[opt_hdr+24], dll_bytes[opt_hdr+25], dll_bytes[opt_hdr+26], dll_bytes[opt_hdr+27],
        dll_bytes[opt_hdr+28], dll_bytes[opt_hdr+29], dll_bytes[opt_hdr+30], dll_bytes[opt_hdr+31],
    ]);
    let entry_rva = u32::from_le_bytes([
        dll_bytes[opt_hdr+16], dll_bytes[opt_hdr+17], dll_bytes[opt_hdr+18], dll_bytes[opt_hdr+19]
    ]) as usize;
    let num_data_dirs = u32::from_le_bytes([
        dll_bytes[opt_hdr+108], dll_bytes[opt_hdr+109], dll_bytes[opt_hdr+110], dll_bytes[opt_hdr+111]
    ]) as usize;

    let sec_start = file_hdr + 20 + size_of_opt_hdr;

    // Allocate via indirect syscall (NtAllocateVirtualMemory)
    let base = match crate::nt_mem::allocate(size_of_image, PAGE_READWRITE) {
        Some(b) => b,
        None => return Err("NtAllocateVirtualMemory failed"),
    };

    crate::helpers::churn(&size_of_image.to_ne_bytes());

    // Copy headers
    ptr::copy_nonoverlapping(dll_bytes.as_ptr(), base, size_of_headers);

    // Map sections
    for i in 0..num_sections {
        let s = sec_start + i * 40;
        let va = u32::from_le_bytes([dll_bytes[s+12], dll_bytes[s+13], dll_bytes[s+14], dll_bytes[s+15]]) as usize;
        let raw_size = u32::from_le_bytes([dll_bytes[s+16], dll_bytes[s+17], dll_bytes[s+18], dll_bytes[s+19]]) as usize;
        let raw_ptr = u32::from_le_bytes([dll_bytes[s+20], dll_bytes[s+21], dll_bytes[s+22], dll_bytes[s+23]]) as usize;

        if raw_size > 0 && raw_ptr + raw_size <= dll_bytes.len() {
            ptr::copy_nonoverlapping(dll_bytes.as_ptr().add(raw_ptr), base.add(va), raw_size);
        }
    }

    crate::helpers::churn(&num_sections.to_ne_bytes());

    // Relocations
    let delta = base as isize - image_base as isize;
    if delta != 0 {
        let data_dirs_off = opt_hdr + 112;
        let reloc_rva = if num_data_dirs > 5 {
            u32::from_le_bytes([dll_bytes[data_dirs_off+40], dll_bytes[data_dirs_off+41], dll_bytes[data_dirs_off+42], dll_bytes[data_dirs_off+43]])
        } else { 0 };
        let reloc_size = if num_data_dirs > 5 {
            u32::from_le_bytes([dll_bytes[data_dirs_off+44], dll_bytes[data_dirs_off+45], dll_bytes[data_dirs_off+46], dll_bytes[data_dirs_off+47]])
        } else { 0 };

        if reloc_rva != 0 && reloc_size != 0 {
            let mut reloc_ptr = base.add(reloc_rva as usize) as *const BaseRelocation;
            while (*reloc_ptr).SizeOfBlock != 0 {
                let block_size = (*reloc_ptr).SizeOfBlock as usize;
                let page_rva = (*reloc_ptr).VirtualAddress as usize;
                let entries = (block_size - 8) / 2;
                let entry_base = (reloc_ptr as *const u8).add(8) as *const u16;

                for j in 0..entries {
                    let entry = entry_base.add(j).read_unaligned();
                    let rtype = entry >> 12;
                    let offset = (entry & 0x0FFF) as usize;
                    let target = base.add(page_rva + offset);

                    match rtype {
                        10 => {
                            let val = (target as *const u64).read_unaligned();
                            (target as *mut u64).write_unaligned(val.wrapping_add(delta as u64));
                        }
                        3 => {
                            let val = (target as *const u32).read_unaligned();
                            (target as *mut u32).write_unaligned(val.wrapping_add(delta as u32));
                        }
                        0 => {}
                        _ => {}
                    }
                }
                reloc_ptr = (reloc_ptr as *const u8).add(block_size) as *const BaseRelocation;
            }
        }

        // Patch ImageBase in mapped headers to prevent CRT double-relocation
        let mapped_opt = base.add(e_lfanew + 4 + 20 + 24) as *mut u64;
        mapped_opt.write_unaligned(base as u64);
    }

    crate::helpers::churn(&delta.to_ne_bytes());

    // Zero bound import directory
    if num_data_dirs > 11 {
        let mapped_dd = base.add(e_lfanew + 4 + 20 + 112) as *mut DataDirectory;
        let bound = &mut *mapped_dd.add(11);
        bound.VirtualAddress = 0;
        bound.Size = 0;
    }

    // Resolve imports
    if num_data_dirs > 1 {
        let mapped_dd = base.add(e_lfanew + 4 + 20 + 112) as *const DataDirectory;
        let import_dir = *mapped_dd.add(1);

        if import_dir.VirtualAddress != 0 {
            let mut desc = base.add(import_dir.VirtualAddress as usize) as *const ImportDescriptor;

            while (*desc).Name != 0 && (*desc).FirstThunk != 0 {
                let dll_name = base.add((*desc).Name as usize);
                let h_module = LoadLibraryA(dll_name);
                if h_module.is_null() {
                    desc = desc.add(1);
                    continue;
                }

                let oft = (*desc).OriginalFirstThunk as usize;
                let ft = (*desc).FirstThunk as usize;
                let ilt_base = if oft != 0 { base.add(oft) } else { base.add(ft) };

                let mut idx = 0usize;
                loop {
                    let thunk = (ilt_base.add(idx * 8) as *const u64).read_unaligned();
                    if thunk == 0 {
                        break;
                    }

                    let func = if thunk & (1u64 << 63) != 0 {
                        let ordinal = (thunk & 0xFFFF) as u16;
                        GetProcAddress(h_module, ordinal as usize as *const u8)
                    } else {
                        let hint_name = base.add(thunk as usize);
                        GetProcAddress(h_module, hint_name.add(2))
                    };

                    let iat_entry = base.add(ft + idx * 8) as *mut u64;
                    iat_entry.write_unaligned(func as u64);

                    idx += 1;
                }
                desc = desc.add(1);
            }
        }
    }

    crate::helpers::churn(&entry_rva.to_ne_bytes());

    // Register exception table (data dir index 3)
    let exception_table = if num_data_dirs > 3 {
        let mapped_dd = base.add(e_lfanew + 4 + 20 + 112) as *const DataDirectory;
        let exc = *mapped_dd.add(3);
        if exc.VirtualAddress != 0 && exc.Size != 0 {
            let table = base.add(exc.VirtualAddress as usize) as *const RuntimeFunction;
            let count = exc.Size / 12;
            RtlAddFunctionTable(table, count, base as u64);
            table
        } else {
            ptr::null()
        }
    } else {
        ptr::null()
    };

    // Initialize security cookie (data dir index 10 = Load Config)
    if num_data_dirs > 10 {
        let mapped_dd = base.add(e_lfanew + 4 + 20 + 112) as *const DataDirectory;
        let lc = *mapped_dd.add(10);
        if lc.VirtualAddress != 0 && lc.Size != 0 {
            let lc_addr = base.add(lc.VirtualAddress as usize);
            let lc_size = (lc_addr as *const u32).read_unaligned() as usize;
            if lc_size >= 104 {
                let cookie_va = (lc_addr.add(96) as *const u64).read_unaligned() as usize;
                if cookie_va != 0 {
                    let tsc: u64;
                    #[cfg(target_arch = "x86_64")]
                    { tsc = core::arch::x86_64::_rdtsc(); }
                    #[cfg(not(target_arch = "x86_64"))]
                    { tsc = 0xDEAD_BEEF_CAFE_BABEu64; }
                    let val = if tsc == 0 || tsc == 0x00002B992DDFA232 { tsc ^ 0xDEADBEEF } else { tsc };
                    (cookie_va as *mut u64).write_unaligned(val);
                }
            }
        }
    }

    // Set section permissions — RWX for all to allow CRT lazy init
    if !crate::nt_mem::protect(base, size_of_image, PAGE_EXECUTE_READWRITE) {
        return Err("NtProtectVirtualMemory failed");
    }

    // Process TLS callbacks (data dir index 9)
    if num_data_dirs > 9 {
        let mapped_dd = base.add(e_lfanew + 4 + 20 + 112) as *const DataDirectory;
        let tls = *mapped_dd.add(9);
        if tls.VirtualAddress != 0 && tls.Size != 0 {
            let tls_addr = base.add(tls.VirtualAddress as usize);
            let addr_of_index = (tls_addr.add(24) as *const u64).read_unaligned() as usize;
            if addr_of_index != 0 {
                (addr_of_index as *mut u32).write_unaligned(0);
            }
            let addr_of_callbacks = (tls_addr.add(32) as *const u64).read_unaligned() as usize;
            if addr_of_callbacks != 0 {
                let mut cb_ptr = addr_of_callbacks as *const u64;
                while (*cb_ptr) != 0 {
                    let callback: DllMainFn = core::mem::transmute(*cb_ptr);
                    callback(base as *mut c_void, 1, ptr::null_mut());
                    cb_ptr = cb_ptr.add(1);
                }
            }
        }
    }

    crate::helpers::churn(&size_of_headers.to_ne_bytes());

    // Call DllMain
    let entry_point = if entry_rva != 0 {
        let ep: DllMainFn = core::mem::transmute(base.add(entry_rva));
        let ret = ep(base as *mut c_void, 1, ptr::null_mut());
        if ret == 0 {
            return Err("DllMain returned 0");
        }
        Some(ep)
    } else {
        None
    };

    Ok(MappedModule {
        base,
        size: size_of_image,
        entry_point,
        exception_table,
    })
}
