use crate::ptrace_control::*;
use nix::sched::*;
use nix::sys::personality;
use nix::unistd::*;
use std::ffi::{CStr, CString};
use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

/// Returns the coverage statistics for a test executable in the given workspace
pub fn launch_program(exe: &Path) -> anyhow::Result<Option<Pid>> {
    if !exe.exists() {
        warn!("Test at {} doesn't exist", exe.display());
        return Ok(None);
    }

    unsafe {
        match fork() {
            Ok(ForkResult::Parent { child }) => Ok(Some(child)),
            Ok(ForkResult::Child) => {
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
    // Create a Command instance with the 'cat' command and the path to the file as arguments
    let output = Command::new("cat")
        .arg("/proc/sys/kernel/random/boot_random")
        .output()
        .unwrap();

    // Convert the output to a String and store it in a variable
    let output_str = String::from_utf8(output.stdout).unwrap();

    // Check if the output string is not '0' (case-insensitive) and return the result
    output_str.trim().to_lowercase() != "0"
}

pub fn execute(test: &Path, argv: &[String], envar: &[(String, String)]) -> anyhow::Result<Pid> {
    let program = CString::new(test.display().to_string()).unwrap_or_default();
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
    execve(&program, &arg_ref, &env_ref)?;

    unreachable!();
}
