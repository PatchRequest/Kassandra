use ntapi::ntldr::LDR_DATA_TABLE_ENTRY;
use ntapi::ntpebteb::PEB;
use ntapi::winapi_local::um::winnt::__readgsqword;
use winapi::shared::basetsd::ULONG_PTR;
use winapi::shared::ntdef::PVOID;
use winapi::um::winnt::{
    IMAGE_DOS_HEADER, IMAGE_DOS_SIGNATURE, IMAGE_EXPORT_DIRECTORY, IMAGE_NT_HEADERS64,
    IMAGE_NT_SIGNATURE,
};
use std::ptr;
const SEED: u32 = 0xEDB88320;
const UP: i32 = -32;
const DOWN: i32 = 32;
const RANGE: u16 = 0xFF;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NtSyscall {
    pub dw_ssn: u32,
    pub dw_syscall_hash: u32,
    pub p_syscall_address: PVOID,
    pub p_syscall_inst_address: PVOID,
}

#[repr(C)]
struct NtdllConfig {
    pdw_array_of_addresses: *const u32,
    pdw_array_of_names: *const u32,
    pw_array_of_ordinals: *const u16,
    dw_number_of_names: u32,
    u_module: ULONG_PTR,
}

static mut G_NTDLL_CONF: NtdllConfig = NtdllConfig {
    pdw_array_of_addresses: std::ptr::null(),
    pdw_array_of_names: std::ptr::null(),
    pw_array_of_ordinals: std::ptr::null(),
    dw_number_of_names: 0,
    u_module: 0,
};

impl Default for NtSyscall {
    fn default() -> Self {
        NtSyscall {
            dw_ssn: 0,
            p_syscall_inst_address: ptr::null_mut(),
            dw_syscall_hash: 0,
            p_syscall_address: ptr::null_mut(),
        }
    }
}


pub fn crc32h(message: &str) -> u32 {
    let g0 = SEED;
    let g1 = g0 >> 1;
    let g2 = g0 >> 2;
    let g3 = g0 >> 3;
    let g4 = g0 >> 4;
    let g5 = g0 >> 5;
    let g6 = (g0 >> 6) ^ g0;
    let g7 = ((g0 >> 6) ^ g0) >> 1;

    let mut crc: i32 = -1;

    for &byte in message.as_bytes() {
        crc ^= byte as i32;

        let c = ((crc << 31) >> 31) as u32 & g7
            ^ ((crc << 30) >> 31) as u32 & g6
            ^ ((crc << 29) >> 31) as u32 & g5
            ^ ((crc << 28) >> 31) as u32 & g4
            ^ ((crc << 27) >> 31) as u32 & g3
            ^ ((crc << 26) >> 31) as u32 & g2
            ^ ((crc << 25) >> 31) as u32 & g1
            ^ ((crc << 24) >> 31) as u32 & g0;

        crc = ((crc as u32) >> 8) as i32 ^ c as i32;
    }

    !(crc as u32)
}

fn init_ntdll_config_structure() -> bool {
    unsafe {
        // let peb: PPEB = std::arch::asm!("mov rax, gs:[0x60]; mov {}, rax", out(reg) _);
        let peb = __readgsqword(0x60) as *const PEB;
        if peb.is_null() || (*peb).OSMajorVersion != 0xA {
            return false;
        }

        let ldr_entry = {
            let first_flink = (*(*peb).Ldr).InMemoryOrderModuleList.Flink;
            let second_flink = (*first_flink).Flink;
            (second_flink as *const u8).offset(-0x10) as *const LDR_DATA_TABLE_ENTRY
        };

        let u_module = (*ldr_entry).DllBase as ULONG_PTR;
        if u_module == 0 {
            return false;
        }

        let dos_header = u_module as *const IMAGE_DOS_HEADER;
        if (*dos_header).e_magic != IMAGE_DOS_SIGNATURE {
            return false;
        }

        let nt_headers = (u_module as *const u8).add((*dos_header).e_lfanew as usize)
            as *const IMAGE_NT_HEADERS64;
        if (*nt_headers).Signature != IMAGE_NT_SIGNATURE {
            return false;
        }

        let export_dir = (u_module as *const u8)
            .add((*nt_headers).OptionalHeader.DataDirectory[0].VirtualAddress as usize)
            as *const IMAGE_EXPORT_DIRECTORY;
        if export_dir.is_null() {
            return false;
        }

        G_NTDLL_CONF.u_module = u_module;
        G_NTDLL_CONF.dw_number_of_names = (*export_dir).NumberOfNames;
        G_NTDLL_CONF.pdw_array_of_names =
            (u_module as *const u8).add((*export_dir).AddressOfNames as usize) as *const u32;
        G_NTDLL_CONF.pdw_array_of_addresses =
            (u_module as *const u8).add((*export_dir).AddressOfFunctions as usize) as *const u32;
        G_NTDLL_CONF.pw_array_of_ordinals =
            (u_module as *const u8).add((*export_dir).AddressOfNameOrdinals as usize) as *const u16;

        if G_NTDLL_CONF.u_module == 0
            || G_NTDLL_CONF.dw_number_of_names == 0
            || G_NTDLL_CONF.pdw_array_of_names.is_null()
            || G_NTDLL_CONF.pdw_array_of_addresses.is_null()
            || G_NTDLL_CONF.pw_array_of_ordinals.is_null()
        {
            return false;
        }
    }

    true
}

pub fn fetch_nt_syscall(dw_sys_hash: u32, nt_sys: &mut NtSyscall) -> bool {
    unsafe {
        if G_NTDLL_CONF.u_module == 0 {
            if !init_ntdll_config_structure() {
                return false;
            }
        }

        if dw_sys_hash != 0 {
            nt_sys.dw_syscall_hash = dw_sys_hash;
        } else {
            return false;
        }

        for i in 0..G_NTDLL_CONF.dw_number_of_names as usize {
            let func_name = {
                let name_rva = *G_NTDLL_CONF.pdw_array_of_names.add(i);
                let name_ptr =
                    (G_NTDLL_CONF.u_module as *const u8).add(name_rva as usize) as *const i8;
                std::ffi::CStr::from_ptr(name_ptr).to_str().unwrap()
            };
            let func_address = (G_NTDLL_CONF.u_module as *const u8).add(
                *G_NTDLL_CONF
                    .pdw_array_of_addresses
                    .add(*G_NTDLL_CONF.pw_array_of_ordinals.add(i) as usize)
                    as usize,
            ) as PVOID;

            if crc32h(func_name) == dw_sys_hash {
                nt_sys.p_syscall_address = func_address;
                let bytes = func_address as *const u8;

                // Check for unhooked syscall
                if *bytes == 0x4C
                    && *bytes.offset(1) == 0x8B
                    && *bytes.offset(2) == 0xD1
                    && *bytes.offset(3) == 0xB8
                    && *bytes.offset(6) == 0x00
                    && *bytes.offset(7) == 0x00
                {
                    let high = *bytes.offset(5);
                    let low = *bytes.offset(4);
                    nt_sys.dw_ssn = ((high as u32) << 8) | low as u32;
                    break;
                }

                // Check for hooked syscall - scenario 1 (0xE9 jump)
                if *bytes == 0xE9 {
                    for idx in 1..=RANGE {
                        let offset_down = (idx as i32 * DOWN) as isize;
                        let offset_up = (idx as i32 * UP) as isize;

                        // Check down
                        if *bytes.offset(offset_down) == 0x4C
                            && *bytes.offset(1 + offset_down) == 0x8B
                            && *bytes.offset(2 + offset_down) == 0xD1
                            && *bytes.offset(3 + offset_down) == 0xB8
                            && *bytes.offset(6 + offset_down) == 0x00
                            && *bytes.offset(7 + offset_down) == 0x00
                        {
                            let high = *bytes.offset(5 + offset_down);
                            let low = *bytes.offset(4 + offset_down);
                            nt_sys.dw_ssn =
                                (((high as u32) << 8) | low as u32).wrapping_sub(idx as u32);
                            break;
                        }

                        // Check up
                        if *bytes.offset(offset_up) == 0x4C
                            && *bytes.offset(1 + offset_up) == 0x8B
                            && *bytes.offset(2 + offset_up) == 0xD1
                            && *bytes.offset(3 + offset_up) == 0xB8
                            && *bytes.offset(6 + offset_up) == 0x00
                            && *bytes.offset(7 + offset_up) == 0x00
                        {
                            let high = *bytes.offset(5 + offset_up);
                            let low = *bytes.offset(4 + offset_up);
                            nt_sys.dw_ssn =
                                (((high as u32) << 8) | low as u32).wrapping_add(idx as u32);
                            break;
                        }
                    }
                }

                // Check for hooked syscall - scenario 2 (0xE9 jump after 3 bytes)
                if *bytes.offset(3) == 0xE9 {
                    for idx in 1..=RANGE {
                        let offset_down = (idx as i32 * DOWN) as isize;
                        let offset_up = (idx as i32 * UP) as isize;

                        // Check down
                        if *bytes.offset(offset_down) == 0x4C
                            && *bytes.offset(1 + offset_down) == 0x8B
                            && *bytes.offset(2 + offset_down) == 0xD1
                            && *bytes.offset(3 + offset_down) == 0xB8
                            && *bytes.offset(6 + offset_down) == 0x00
                            && *bytes.offset(7 + offset_down) == 0x00
                        {
                            let high = *bytes.offset(5 + offset_down);
                            let low = *bytes.offset(4 + offset_down);
                            nt_sys.dw_ssn =
                                (((high as u32) << 8) | low as u32).wrapping_sub(idx as u32);
                            break;
                        }

                        // check up ....
                        if *bytes.offset(offset_up) == 0x4C
                            && *bytes.offset(1 + offset_up) == 0x8B
                            && *bytes.offset(2 + offset_up) == 0xD1
                            && *bytes.offset(3 + offset_up) == 0xB8
                            && *bytes.offset(6 + offset_up) == 0x00
                            && *bytes.offset(7 + offset_up) == 0x00
                        {
                            let high = *bytes.offset(5 + offset_up);
                            let low = *bytes.offset(4 + offset_up);
                            nt_sys.dw_ssn =
                                (((high as u32) << 8) | low as u32).wrapping_add(idx as u32);
                            break;
                        }
                    }
                }

                break;
            }
        }

        if nt_sys.p_syscall_address.is_null() {
            return false;
        }

        // looking for a syscall instruction in a neighboring function
        let u_func_address = (nt_sys.p_syscall_address as ULONG_PTR) + 0xFF;
        for z in 0..=RANGE as u32 {
            let x = z + 1;
            let bytes = u_func_address as *const u8;
            if *bytes.offset(z as isize) == 0x0F && *bytes.offset(x as isize) == 0x05 {
                nt_sys.p_syscall_inst_address = (u_func_address + z as usize) as PVOID;
                break;
            }
        }

        nt_sys.dw_ssn != 0
            && !nt_sys.p_syscall_address.is_null()
            && nt_sys.dw_syscall_hash != 0
            && !nt_sys.p_syscall_inst_address.is_null()
    }
}

unsafe extern "C" {
    pub fn SetSSn(w_system_call: u16, syscall_inst_address: PVOID);
    pub fn RunSyscall(
        arg1: *mut winapi::ctypes::c_void,
        arg2: *mut winapi::ctypes::c_void,
        arg3: *mut winapi::ctypes::c_void,
        arg4: *mut winapi::ctypes::c_void,
        arg5: *mut winapi::ctypes::c_void,
        arg6: *mut winapi::ctypes::c_void,
        arg7: *mut winapi::ctypes::c_void,
        arg8: *mut winapi::ctypes::c_void,
        arg9: *mut winapi::ctypes::c_void,
        arg10: *mut winapi::ctypes::c_void,
        arg11: *mut winapi::ctypes::c_void,
    ) -> winapi::shared::ntdef::NTSTATUS;
}
