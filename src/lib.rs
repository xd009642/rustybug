use crate::linux::launch_program;
use clap::Parser;
use nix::errno::Errno;
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::wait::*;
use nix::unistd::Pid;
use nix::Error as NixErr;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{debug, info};

pub mod breakpoint;
//pub mod test_loader;
pub mod linux;
pub mod ptrace_control;

/// rustybug a moderately simple debugger written in rust. Not intended to be feature complete more
/// a toy project and way to test some tarpaulin assumptions.
#[derive(Clone, Debug, Default, Parser)]
pub struct Args {
    /// Executable to debug
    pub input: Option<PathBuf>,
    /// PID of a running process to attach to
    #[clap(long, short)]
    pub pid: Option<i32>,
}

impl Args {
    pub fn name(&self) -> String {
        if let Some(input) = self.input.as_ref() {
            input.display().to_string()
        } else if let Some(pid) = self.pid {
            format!("pid: {}", pid)
        } else {
            "No Attached Process".to_string()
        }
    }

    pub fn set_input(&mut self, input: PathBuf) {
        self.input = Some(input);
        self.pid = None;
    }

    pub fn set_pid(&mut self, input: i32) {
        self.pid = Some(input);
        self.input = None;
    }
}

#[derive(Debug)]
pub struct DebuggerStateMachine {
    root: Pid,
    args: Args,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    Finished,
}

impl DebuggerStateMachine {
    pub fn start(args: Args) -> anyhow::Result<Self> {
        let pid = if let Some(input) = args.input.as_ref() {
            launch_program(input)?.unwrap()
        } else if let Some(pid) = args.pid {
            let pid = Pid::from_raw(pid);
            ptrace::attach(pid).unwrap();
            pid
        } else {
            panic!("You should provide an executable name or PID");
        };

        let waiting = Instant::now();
        let timeout = Duration::from_secs(15);

        info!(pid=?pid, "program launch. Continuing");

        while waiting.elapsed() < timeout {
            match waitpid(pid, Some(WaitPidFlag::WNOHANG))? {
                WaitStatus::StillAlive => {}
                sig @ WaitStatus::Stopped(_, Signal::SIGTRAP) => {
                    debug!("We're free running!");
                    ptrace_control::continue_exec(pid, None)?;
                    break;
                }
                unexpected => anyhow::bail!("Unexpected signal: {:?}", unexpected),
            }
        }

        Ok(Self { root: pid, args })
    }

    pub fn wait(&mut self) -> anyhow::Result<State> {
        match waitpid(self.root, Some(WaitPidFlag::WNOHANG | WaitPidFlag::__WALL))? {
            WaitStatus::StillAlive => Ok(State::Running),
            WaitStatus::Exited(child, ret_code) => {
                if child == self.root {
                    info!("Process {:?} exited with exit code {}", child, ret_code);
                    Ok(State::Finished)
                } else {
                    Ok(State::Running)
                }
            }
            _ => unimplemented!(),
        }
    }
}
