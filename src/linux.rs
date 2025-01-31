use crate::ptrace_control::*;
use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::sys::personality;
use nix::unistd::*;
use std::ffi::{CStr, CString};
use std::io;
use std::os::fd::OwnedFd;
use std::path::Path;
use std::process::Command;
use tracing::warn;

pub struct LaunchedProcess {
    pub pid: Pid,
    pub stdout_reader: Option<OwnedFd>,
}

/// This is in nix but not yet released on crates.io so should be able to remove it in 0.30.0
#[inline]
pub fn dup2_stdout<Fd: std::os::fd::AsFd>(fd: Fd) -> Result<(), Errno> {
    use libc::STDOUT_FILENO;
    use std::os::fd::AsRawFd;

    let res = unsafe { libc::dup2(fd.as_fd().as_raw_fd(), STDOUT_FILENO) };
    Errno::result(res).map(drop)
}

/// Returns the coverage statistics for a test executable in the given workspace
pub fn launch_program(exe: &Path) -> anyhow::Result<Option<LaunchedProcess>> {
    if !exe.exists() {
        warn!("Test at {} doesn't exist", exe.display());
        return Ok(None);
    }

    let (read, write) = pipe2(OFlag::O_CLOEXEC)?;

    unsafe {
        match fork() {
            Ok(ForkResult::Parent { child }) => Ok(Some(LaunchedProcess {
                pid: child,
                stdout_reader: Some(read),
            })),
            Ok(ForkResult::Child) => {
                std::mem::drop(read);
                /*if let Err(e) = dup2_stdout(&write) {
                    warn!("Failed to redirect stdout");
                }*/
                execute(exe, &[], &[])?;
                Ok(None)
            }
            Err(err) => anyhow::bail!("Failed to run test {}, Error: {}", exe.display(), err),
        }
    }
}

fn disable_aslr() -> nix::Result<()> {
    let this = personality::get()?;
    personality::set(this | personality::Persona::ADDR_NO_RANDOMIZE).map(|_| ())
}

fn is_aslr_enabled() -> bool {
    !personality::get()
        .map(|x| x.contains(personality::Persona::ADDR_NO_RANDOMIZE))
        .unwrap_or(true)
}

#[cfg(not(tarpaulin_include))]
pub fn execute(test: &Path, argv: &[String], envar: &[(String, String)]) -> anyhow::Result<Pid> {
    let program = CString::new(test.display().to_string()).unwrap_or_default();
    if let Err(e) = setpgid(Pid::from_raw(0), Pid::from_raw(0)) {
        warn!("Failed to set pgid: {}", e);
    }
    if is_aslr_enabled() {
        disable_aslr()?;
    }
    request_trace()?;

    let envar = envar
        .iter()
        .map(|(k, v)| CString::new(format!("{k}={v}").as_str()).unwrap_or_default())
        .collect::<Vec<CString>>();

    let argv = argv
        .iter()
        .map(|x| CString::new(x.as_str()).unwrap_or_default())
        .collect::<Vec<CString>>();

    let arg_ref = argv.iter().map(AsRef::as_ref).collect::<Vec<&CStr>>();
    let env_ref = envar.iter().map(AsRef::as_ref).collect::<Vec<&CStr>>();

    // TODO if I wanted to be _extra_ try hard for tarpaulin I'd see if I could manually trigger a
    // saving of a profraw file here before execve!

    execve(&program, &arg_ref, &env_ref)?;

    unreachable!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_disable_aslr() {
        assert!(is_aslr_enabled());
        disable_aslr().unwrap();
        assert!(!is_aslr_enabled());
    }
}
