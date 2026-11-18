//! The processes running on this machine, each with its name and arguments.
//!
//! Linux has them under `/proc`. FreeBSD has a `/proc` only when procfs is
//! mounted, which it is not by default, and its procfs has no `comm`, so there
//! they come from `sysctl` instead.

pub struct Process {
    pub pid: i32,
    /// The kernel's short name for the process, as `comm` shows it.
    pub name: String,
    pub args: Vec<String>,
}

/// Arguments as the kernel keeps them: one after another, each ended by a NUL.
fn split_args(raw: &[u8]) -> Vec<String> {
    raw.split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect()
}

#[cfg(not(target_os = "freebsd"))]
pub fn all() -> Vec<Process> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let pid = e.file_name().to_str()?.parse().ok()?;
            let name = std::fs::read_to_string(e.path().join("comm")).unwrap_or_default();
            let args = split_args(&std::fs::read(e.path().join("cmdline")).ok()?);
            Some(Process {
                pid,
                name: name.trim().to_string(),
                args,
            })
        })
        .collect()
}

#[cfg(target_os = "freebsd")]
pub fn all() -> Vec<Process> {
    freebsd::all()
}

#[cfg(target_os = "freebsd")]
mod freebsd {
    use std::ffi::{c_int, c_uint, c_void};

    use super::{Process, split_args};

    unsafe extern "C" {
        fn sysctl(
            name: *const c_int,
            namelen: c_uint,
            oldp: *mut c_void,
            oldlenp: *mut usize,
            newp: *const c_void,
            newlen: usize,
        ) -> c_int;
    }

    const CTL_KERN: c_int = 1;
    const KERN_PROC: c_int = 14;
    const KERN_PROC_ARGS: c_int = 7;
    const KERN_PROC_PROC: c_int = 8;
    /// Where `ki_pid` and `ki_comm` are in `struct kinfo_proc`, whose layout
    /// FreeBSD keeps fixed (`KINFO_PROC_SIZE`); `ki_comm` holds `COMMLEN` + 1
    /// bytes.
    const KI_PID: usize = 72;
    const KI_COMM: usize = 447;
    const COMM_SIZE: usize = 20;

    /// What `sysctl` answers for `mib`, retried while what it answers grows
    /// between asking for the size and reading it.
    fn read(mib: &[c_int]) -> Option<Vec<u8>> {
        for _ in 0..4 {
            let mut len = 0usize;
            let rc = unsafe {
                sysctl(
                    mib.as_ptr(),
                    mib.len() as c_uint,
                    std::ptr::null_mut(),
                    &mut len,
                    std::ptr::null(),
                    0,
                )
            };
            if rc != 0 {
                return None;
            }
            // Room for processes started in between.
            len += len / 8;
            let mut buf = vec![0u8; len];
            let rc = unsafe {
                sysctl(
                    mib.as_ptr(),
                    mib.len() as c_uint,
                    buf.as_mut_ptr().cast(),
                    &mut len,
                    std::ptr::null(),
                    0,
                )
            };
            if rc == 0 {
                buf.truncate(len);
                return Some(buf);
            }
            if std::io::Error::last_os_error().raw_os_error() != Some(12) {
                return None;
            }
        }
        None
    }

    fn int_at(buf: &[u8], at: usize) -> Option<i32> {
        Some(i32::from_ne_bytes(buf.get(at..at + 4)?.try_into().ok()?))
    }

    pub fn all() -> Vec<Process> {
        let Some(table) = read(&[CTL_KERN, KERN_PROC, KERN_PROC_PROC, 0]) else {
            return Vec::new();
        };
        let mut processes = Vec::new();
        let mut at = 0;
        // Each entry starts with its own size, `ki_structsize`.
        while let Some(size) = int_at(&table, at) {
            let size = size as usize;
            let Some(entry) = table.get(at..at + size).filter(|_| size > KI_COMM) else {
                break;
            };
            at += size;
            let Some(pid) = int_at(entry, KI_PID) else {
                continue;
            };
            let comm = &entry[KI_COMM..(KI_COMM + COMM_SIZE).min(size)];
            let comm = comm.split(|b| *b == 0).next().unwrap_or_default();
            // A process that has exited, or is another user's and hidden by
            // security.bsd.see_other_uids, has no arguments to read.
            let Some(args) = read(&[CTL_KERN, KERN_PROC, KERN_PROC_ARGS, pid]) else {
                continue;
            };
            processes.push(Process {
                pid,
                name: String::from_utf8_lossy(comm).into_owned(),
                args: split_args(&args),
            });
        }
        processes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_this_process_with_its_arguments() {
        let me = std::process::id() as i32;
        let found = all()
            .into_iter()
            .find(|p| p.pid == me)
            .expect("this process is listed");
        let exe = std::env::args().next().unwrap();
        assert_eq!(found.args.first(), Some(&exe));
        assert!(!found.name.is_empty());
        assert!(exe.rsplit('/').next().unwrap().starts_with(&found.name));
    }
}
