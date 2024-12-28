use crate::process::Process;
use clap::Parser;
use nix::unistd::Pid;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::info;

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

        info!(pid=?root.pid(), "program launch. Continuing");

        Ok(Self { root, args })
    }

    pub fn wait(&mut self) -> anyhow::Result<State> {
        self.root.wait_on_signal()?;
        Ok(self.root.state())
    }

    pub fn cont(&mut self) -> anyhow::Result<()> {
        if self.root.state() == State::Stopped {
            self.root.resume()?;
        }
        Ok(())
    }
}
