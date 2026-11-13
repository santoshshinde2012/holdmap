//! Windows memory counters from `NtQueryInformationProcess` (`ProcessVmCounters`)
//! and `GetProcessMemoryInfo`.
//!
//! The working set matches Task Manager's older "working set" figure and what sysinfo
//! stores. The private working set (Task Manager's Memory column on current Windows) is
//! not returned by those documented structs, so it stays unknown. `PrivateUsage` is the
//! private commit charge and is reported as commit, not as private memory.
//! Memory compression is a machine-wide store without a per-process counter here.

use super::figure::ObservedU64;
use super::probe::ProcessReading;
use std::mem::{size_of, MaybeUninit};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

const PROCESS_VM_COUNTERS: u32 = 3;

#[repr(C)]
struct VmCounters {
    peak_virtual_size: usize,
    virtual_size: usize,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        process: HANDLE,
        class: u32,
        information: *mut core::ffi::c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

pub(super) fn read(pid: u32) -> ProcessReading {
    if pid == 0 {
        return ProcessReading::all_unknown("Process id 0 has no Windows memory counters.");
    }
    // SAFETY: a null handle is a failed open; the last error is then a reason, not a reading.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return ProcessReading::all_unknown(format!(
            "Opening the process for a memory query failed ({})",
            io_reason()
        ));
    }
    let handle = Handle(handle);
    let mut vm = MaybeUninit::<VmCounters>::zeroed();
    let mut returned = 0u32;
    // SAFETY: vm is the documented VM_COUNTERS layout and the handle is open.
    let status = unsafe {
        NtQueryInformationProcess(
            handle.0,
            PROCESS_VM_COUNTERS,
            vm.as_mut_ptr().cast(),
            size_of::<VmCounters>() as u32,
            &mut returned,
        )
    };
    let vm = if status == 0 {
        // SAFETY: NTSTATUS 0 means the struct was written.
        Some(unsafe { vm.assume_init() })
    } else {
        None
    };
    let mut counters = MaybeUninit::<PROCESS_MEMORY_COUNTERS_EX>::zeroed();
    // SAFETY: the struct starts with cb, which GetProcessMemoryInfo requires.
    unsafe {
        (*counters.as_mut_ptr()).cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    }
    // SAFETY: handle is open and counters points at the EX struct whose cb was set.
    let got_ex = unsafe {
        GetProcessMemoryInfo(
            handle.0,
            counters.as_mut_ptr().cast(),
            size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        )
    } != 0;
    let counters = got_ex.then(|| unsafe { counters.assume_init() });
    if vm.is_none() && counters.is_none() {
        return ProcessReading::all_unknown(
            "Neither the process VM counters nor GetProcessMemoryInfo returned data.",
        );
    }
    let working_set = vm
        .as_ref()
        .map(|vm| vm.working_set_size as u64)
        .or_else(|| counters.as_ref().map(|c| c.WorkingSetSize as u64));
    let peak_working_set = vm
        .as_ref()
        .map(|vm| vm.peak_working_set_size as u64)
        .or_else(|| counters.as_ref().map(|c| c.PeakWorkingSetSize as u64));
    let virtual_size = vm.as_ref().map(|vm| vm.virtual_size as u64);
    let commit = counters
        .as_ref()
        .map(|c| c.PrivateUsage as u64)
        .or_else(|| vm.as_ref().map(|vm| vm.pagefile_usage as u64));
    let from = |value: Option<u64>, missing: &str| {
        value
            .map(ObservedU64::observed)
            .unwrap_or_else(|| ObservedU64::unknown(missing))
    };
    ProcessReading {
        resident: from(working_set, "The working set was not in the process counters."),
        physical_footprint: ObservedU64::unknown(
            "Windows has no physical-footprint counter. The working set is the resident figure holdmap shows.",
        ),
        private_bytes: ObservedU64::unknown(
            "GetProcessMemoryInfo does not return the private working set. PrivateUsage is commit charge and is reported separately, not as private memory.",
        ),
        shared_bytes: ObservedU64::unknown(
            "Shared working set is working set minus private working set. The private working set was not returned, so shared memory is not derived.",
        ),
        proportional: ObservedU64::unknown(
            "Proportional set size is a Linux smaps counter. Windows does not report it.",
        ),
        compressed: ObservedU64::unknown(
            "Windows memory compression is not exposed as a per-process counter by these process queries.",
        ),
        virtual_bytes: from(virtual_size, "Virtual size was not in the process VM counters."),
        swap_bytes: ObservedU64::unknown(
            "Windows does not report per-process swap through these process queries.",
        ),
        peak_resident: from(
            peak_working_set,
            "Peak working set was not in the process counters.",
        ),
        peak_physical: ObservedU64::unknown(
            "Windows does not report a peak physical footprint. Peak working set is reported as peak resident.",
        ),
        commit_bytes: from(commit, "Private commit charge was not in the process counters."),
    }
}

pub(super) fn machine_bytes() -> ObservedU64 {
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        dwMemoryLoad: 0,
        ullTotalPhys: 0,
        ullAvailPhys: 0,
        ullTotalPageFile: 0,
        ullAvailPageFile: 0,
        ullTotalVirtual: 0,
        ullAvailVirtual: 0,
        ullAvailExtendedVirtual: 0,
    };
    // SAFETY: status.dwLength is set and the pointer is a writable MEMORYSTATUSEX.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
    if !ok || status.ullTotalPhys == 0 {
        return ObservedU64::unknown(
            "GlobalMemoryStatusEx did not return the machine's physical memory size.",
        );
    }
    ObservedU64::observed(status.ullTotalPhys)
}

fn io_reason() -> &'static str {
    match std::io::Error::last_os_error().kind() {
        std::io::ErrorKind::PermissionDenied => "access denied",
        std::io::ErrorKind::NotFound => "process not found",
        _ => "the query failed",
    }
}
