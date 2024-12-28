use crate::linux::launch_program;
use crate::process::Process;
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

pub use crate::process::State;

pub mod breakpoint;
pub mod commands;
//pub mod test_loader;
pub mod linux;
pub mod process;
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
    root: Process,
    paused: Vec<Pid>,
    args: Args,
}

impl DebuggerStateMachine {
    pub fn start(args: Args) -> anyhow::Result<Self> {
        let mut root = if let Some(input) = args.input.as_ref() {
            Process::launch(input)?
        } else if let Some(pid) = args.pid {
            let pid = Pid::from_raw(pid);
            Process::attach(pid)?
        } else {
            panic!("You should provide an executable name or PID");
        };

        let waiting = Instant::now();
        let timeout = Duration::from_secs(15);

        info!(pid=?root.pid(), "program launch. Continuing");

        let mut paused = vec![];

        while waiting.elapsed() < timeout {
            let pid = root.wait_on_signal()?;
            if let Some(pid) = pid {
                paused.push(pid);
                if root.pid() == pid {
                    break;
                }
            }
        }

        Ok(Self { root, paused, args })
    }

    pub fn wait(&mut self) -> anyhow::Result<State> {
        self.root.wait_on_signal()?;
        Ok(self.root.state())
    }

    pub fn cont(&mut self) -> anyhow::Result<()> {
        for pid in self.paused.drain(..) {
            ptrace_control::continue_exec(pid, None)?;
        }
        Ok(())
    }
}
