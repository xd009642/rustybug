use crate::linux::launch_program;
use crate::ptrace_control::*;
use nix::errno::Errno;
use nix::sys::ptrace;
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::*;
use nix::unistd::Pid;
use nix::Error as NixErr;
use std::path::Path;
use thiserror::Error;
use tracing::{error, info, warn};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum State {
    Stopped,
    Running,
    Exited,
    Terminated,
}

impl State {
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Exited | Self::Terminated)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Error)]
pub enum ProcessError {
    #[error("process pid is unknown")]
    NoPid,
    #[error("failed to launch process")]
    LaunchFailed,
    #[error("failed to attach to process")]
    AttachFailed,
    #[error("failed to wait on the process pid")]
    WaitFailed,
}

#[derive(Debug)]
pub struct Process {
    pid: Pid,
    terminate_on_end: bool,
    state: State,
}

impl Process {
    pub fn launch(path: &Path) -> Result<Self, ProcessError> {
        let pid = launch_program(path)
            .map_err(|e| {
                error!("Failed to launch: {}", e);
                ProcessError::LaunchFailed
            })?
            .ok_or(ProcessError::NoPid)?;

        Ok(Self {
            pid,
            terminate_on_end: true,
            state: State::Stopped,
        })
    }

    pub fn attach(pid: Pid) -> Result<Self, ProcessError> {
        ptrace::attach(pid).map_err(|e| {
            error!("Failed to attach: {}", e);
            ProcessError::AttachFailed
        })?;
        Ok(Self {
            pid,
            terminate_on_end: false,
            state: State::Stopped,
        })
    }

    pub fn resume(&self) -> Result<Self, ProcessError> {
        todo!()
    }

    pub fn pid(&self) -> Pid {
        self.pid
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn wait_on_signal(&mut self) -> Result<Option<Pid>, ProcessError> {
        let mut ret = None;
        let state = match waitpid(self.pid, Some(WaitPidFlag::WNOHANG))
            .map_err(|_| ProcessError::WaitFailed)?
        {
            WaitStatus::StillAlive => State::Running,
            sig @ WaitStatus::Exited(child, ret_code) => {
                if child == self.pid {
                    info!("Process {:?} exited with exit code {}", child, ret_code);
                    State::Exited
                } else {
                    State::Running
                }
            }
            WaitStatus::Stopped(child, Signal::SIGTRAP) => {
                ret = Some(child);
                State::Stopped
            }
            _ => unimplemented!(),
        };
        self.state = state;
        Ok(ret)
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if self.pid.as_raw() != 0 {
            if self.state == State::Running {
                if let Err(e) = kill(self.pid, Signal::SIGSTOP) {
                    warn!("Sending sigstop to process on teardown failed: {}", e);
                }
                if let Err(e) = waitpid(self.pid, None) {
                    warn!("Couldn't wait on receiving stop: {}", e);
                }
            }

            if let Err(e) = detach_child(self.pid) {
                warn!("Failed to detach on teardown: {}", e);
            }
            if let Err(e) = kill(self.pid, Signal::SIGCONT) {
                warn!("Couldn't continue after detach: {}", e);
            }

            if self.terminate_on_end {
                if let Err(e) = kill(self.pid, Signal::SIGKILL) {
                    warn!("Couldn't issue sigkill on teardown: {}", e);
                }
                if let Err(e) = waitpid(self.pid, None) {
                    warn!("Wait after sigkill failed: {}", e);
                }
            }
        }
    }
}
