use crate::linux::launch_program;
use clap::Parser;
use nix::errno::Errno;
use nix::sys::signal::Signal;
use nix::sys::wait::*;
use nix::unistd::Pid;
use nix::Error as NixErr;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::info;

pub mod breakpoint;
//pub mod test_loader;
pub mod linux;
pub mod ptrace_control;

/// rustybug a moderately simple debugger written in rust. Not intended to be feature complete more
/// a toy project and way to test some tarpaulin assumptions.
#[derive(Debug, Parser)]
pub struct Args {
    /// Executable to debug
    pub input: PathBuf,
}

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
        let pid = launch_program(&args.input)?.unwrap();

        let waiting = Instant::now();
        let timeout = Duration::from_secs(15);

        info!(pid=?pid, "program launch. Continuing");

        while waiting.elapsed() < timeout {
            match waitpid(pid, Some(WaitPidFlag::WNOHANG))? {
                WaitStatus::StillAlive => {}
                sig @ WaitStatus::Stopped(_, Signal::SIGTRAP) => {
                    println!("We're free running!");
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
