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

/// A command for a program found on PATH, named by its full path.
///
/// Given a bare name, Rust's `Command` has `posix_spawnp` search PATH in the
/// child. On FreeBSD that child runs in Ginrose's own memory until it execs,
/// and from the GTK process it corrupted the heap: the launcher crashed as
/// soon as the settings page asked xdg-mime who opens Roblox links. A full
/// path takes `posix_spawn`, which has nothing to search. A program that is
/// not on PATH keeps its bare name and fails to start as it always did.
pub fn command(program: &str) -> std::process::Command {
    let found = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|candidate| is_executable(candidate))
    });
    std::process::Command::new(found.as_deref().unwrap_or(program.as_ref()))
}

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_absolute()
        && std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
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
    use super::{Process, split_args};

    /// What `sysctl` answers for `mib`, retried while what it answers grows
    /// between asking for the size and reading it.
    fn read(mib: &[libc::c_int]) -> Option<Vec<u8>> {
        for _ in 0..4 {
            let mut len = 0usize;
            // SAFETY: a size query: no buffer, and `len` is live.
            let rc = unsafe {
                libc::sysctl(
                    mib.as_ptr(),
                    mib.len() as libc::c_uint,
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
            // SAFETY: a buffer of `len` bytes, which `len` says.
            let rc = unsafe {
                libc::sysctl(
                    mib.as_ptr(),
                    mib.len() as libc::c_uint,
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
            if std::io::Error::last_os_error().raw_os_error() != Some(libc::ENOMEM) {
                return None;
            }
        }
        None
    }

    pub fn all() -> Vec<Process> {
        let mib = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PROC, 0];
        let Some(table) = read(&mib) else {
            return Vec::new();
        };
        let size = std::mem::size_of::<libc::kinfo_proc>();
        // Each entry starts with its own size; one that is not the struct's
        // means a kernel this was not built for.
        let first = table
            .get(..4)
            .map(|b| i32::from_ne_bytes(b.try_into().unwrap()));
        if first.is_some_and(|first| first as usize != size) {
            return Vec::new();
        }
        table
            .chunks_exact(size)
            .filter_map(|entry| {
                // SAFETY: `size` bytes the kernel wrote as a `kinfo_proc`,
                // which may not be aligned in the buffer.
                let proc = unsafe { entry.as_ptr().cast::<libc::kinfo_proc>().read_unaligned() };
                // A process that has exited, or is another user's and hidden
                // by security.bsd.see_other_uids, has no arguments to read.
                let mib = [
                    libc::CTL_KERN,
                    libc::KERN_PROC,
                    libc::KERN_PROC_ARGS,
                    proc.ki_pid,
                ];
                let args = read(&mib)?;
                let comm: Vec<u8> = proc
                    .ki_comm
                    .iter()
                    .take_while(|c| **c != 0)
                    .map(|c| *c as u8)
                    .collect();
                Some(Process {
                    pid: proc.ki_pid,
                    name: String::from_utf8_lossy(&comm).into_owned(),
                    args: split_args(&args),
                })
            })
            .collect()
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

    #[test]
    fn names_a_program_on_path_by_its_full_path() {
        let sh = command("sh");
        let program = std::path::Path::new(sh.get_program());
        assert!(program.is_absolute(), "{}", program.display());
        assert!(sh.get_program().to_string_lossy().ends_with("/sh"));
        assert_eq!(
            command("no-such-program-anywhere").get_program(),
            "no-such-program-anywhere"
        );
    }
}
