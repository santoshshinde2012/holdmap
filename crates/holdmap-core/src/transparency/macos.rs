//! macOS memory counters from `proc_pid_rusage` and `proc_pidinfo`.
//!
//! Activity Monitor's Memory column is `ri_phys_footprint`. Resident size and the lifetime
//! peak footprint come from the same call (`RUSAGE_INFO_V4`, which includes the V0 fields).
//! Virtual size is `pti_virtual_size` from `proc_pidinfo(PROC_PIDTASKINFO)`, the call `ps` and
//! Activity Monitor use (`struct proc_taskinfo` in xnu's `bsd/sys/proc_info.h`).
//! Compressed, private and shared bytes are the columns Activity Monitor reads through
//! `task_info`, and that needs `task_for_pid`. holdmap does not request that privilege, so
//! those counters stay unknown instead of being guessed from the footprint.

use super::figure::ObservedU64;
use super::probe::ProcessReading;
use std::mem::MaybeUninit;

pub(super) fn read(pid: u32) -> ProcessReading {
    if pid == 0 || pid > i32::MAX as u32 {
        return ProcessReading::all_unknown(
            "proc_pid_rusage only accepts a process id that fits in a positive int.",
        );
    }
    let mut info = MaybeUninit::<libc::rusage_info_v4>::zeroed();
    // SAFETY: the buffer is the V4 layout libc declares, and the kernel writes at most that
    // struct for RUSAGE_INFO_V4. On failure the buffer is ignored.
    let rc = unsafe {
        libc::proc_pid_rusage(
            pid as libc::c_int,
            libc::RUSAGE_INFO_V4,
            info.as_mut_ptr().cast(),
        )
    };
    if rc != 0 {
        let reason = std::io::Error::last_os_error();
        return ProcessReading::all_unknown(format!(
            "proc_pid_rusage failed ({})",
            match reason.kind() {
                std::io::ErrorKind::PermissionDenied => "access denied",
                _ => "the call was rejected",
            }
        ));
    }
    // SAFETY: proc_pid_rusage returned success, so the V4 struct is initialised.
    let info = unsafe { info.assume_init() };
    let unavailable = || {
        ObservedU64::unknown(
            "proc_pid_rusage (RUSAGE_INFO_V4) does not return this counter. task_for_pid is not used, because it needs a privilege holdmap does not request.",
        )
    };
    ProcessReading {
        resident: ObservedU64::observed(info.ri_resident_size),
        physical_footprint: ObservedU64::observed(info.ri_phys_footprint),
        private_bytes: unavailable(),
        shared_bytes: unavailable(),
        proportional: ObservedU64::unknown(
            "Proportional set size is a Linux smaps counter. macOS does not report it.",
        ),
        compressed: unavailable(),
        virtual_bytes: virtual_size(pid as libc::c_int),
        swap_bytes: ObservedU64::unknown(
            "macOS proc_pid_rusage does not report per-process swap. Compressed memory, which Activity Monitor shows separately, is also not in this call.",
        ),
        peak_resident: ObservedU64::unknown(
            "proc_pid_rusage reports a lifetime peak physical footprint, not a peak resident size.",
        ),
        peak_physical: ObservedU64::observed(info.ri_lifetime_max_phys_footprint),
        commit_bytes: ObservedU64::unknown(
            "Commit charge is a Windows counter. macOS does not report it.",
        ),
    }
}

/// Virtual address-space size from the task info `ps` reads. Other users' processes refuse it.
fn virtual_size(pid: libc::c_int) -> ObservedU64 {
    let mut info = MaybeUninit::<libc::proc_taskinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
    // SAFETY: the buffer is the proc_taskinfo layout libc declares, and `size` tells the kernel
    // how much it may write. A short or failed write leaves the buffer unused.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if written != size {
        return ObservedU64::unknown(
            "proc_pidinfo (PROC_PIDTASKINFO) was refused for this process, as it is for other users' processes.",
        );
    }
    // SAFETY: the kernel wrote the whole struct.
    ObservedU64::observed(unsafe { info.assume_init() }.pti_virtual_size)
}

pub(super) fn machine_bytes() -> ObservedU64 {
    let mut bytes: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    let name = c"hw.memsize";
    // SAFETY: name is a nul-terminated literal, and bytes/len describe a u64 out-buffer.
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut bytes as *mut u64).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || len != std::mem::size_of::<u64>() || bytes == 0 {
        return ObservedU64::unknown(
            "sysctl hw.memsize did not return the machine's physical memory size.",
        );
    }
    ObservedU64::observed(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_processs_virtual_size_and_keeps_privileged_counters_unknown() {
        let reading = read(std::process::id());
        let virtual_bytes = reading
            .virtual_bytes
            .get()
            .expect("own task info is readable");
        let resident = reading.resident.get().expect("own rusage is readable");
        assert!(virtual_bytes >= resident, "{virtual_bytes} < {resident}");
        assert!(reading.compressed.get().is_none());
        assert!(reading.private_bytes.get().is_none());
        // launchd belongs to root; without privileges its task info is refused, never guessed.
        if unsafe { libc::geteuid() } != 0 {
            assert!(virtual_size(1).get().is_none());
        }
    }
}
