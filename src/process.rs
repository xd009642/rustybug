use crate::linux::launch_program;
use crate::ptrace_control::*;
use nix::sys::ptrace;
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::*;
use nix::unistd::Pid;
use std::path::Path;
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{error, info, warn};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct StopReason {
    reason: State,
    info: Info,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Info {
    Signalled(Signal),
    Return(u8),
}

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
    #[error("failed to resume process")]
    ContinueFailed,
    #[error("blocking operation timed out")]
    Timeout,
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

        let mut ret = Self {
            pid,
            terminate_on_end: true,
            state: State::Stopped,
        };

        let timeout = Duration::from_secs(15);
        ret.blocking_wait_on_signal(timeout)?;

        Ok(ret)
    }

    pub fn attach(pid: Pid) -> Result<Self, ProcessError> {
        ptrace::attach(pid).map_err(|e| {
            error!("Failed to attach: {}", e);
            ProcessError::AttachFailed
        })?;
        let mut ret = Self {
            pid,
            terminate_on_end: false,
            state: State::Stopped,
        };

        let timeout = Duration::from_secs(15);
        ret.blocking_wait_on_signal(timeout)?;

        Ok(ret)
    }

    pub fn resume(&mut self) -> Result<(), ProcessError> {
        continue_exec(self.pid, None).map_err(|_| ProcessError::ContinueFailed)?;
        self.state = State::Running;
        Ok(())
    }

    pub fn pid(&self) -> Pid {
        self.pid
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn blocking_wait_on_signal(
        &mut self,
        timeout: Duration,
    ) -> Result<StopReason, ProcessError> {
        let waiting = Instant::now();
        while waiting.elapsed() < timeout {
            if let Some(res) = self.wait_on_signal()? {
                return Ok(res);
            }
        }
        Err(ProcessError::Timeout)
    }

    pub fn wait_on_signal(&mut self) -> Result<Option<StopReason>, ProcessError> {
        let mut ret = None;
        let state = match waitpid(self.pid, Some(WaitPidFlag::WNOHANG))
            .map_err(|_| ProcessError::WaitFailed)?
        {
            WaitStatus::StillAlive => State::Running,
            sig @ WaitStatus::Exited(child, ret_code) => {
                ret = Some(StopReason {
                    reason: State::Exited,
                    info: Info::Return(ret_code as u8),
                });
                if child == self.pid {
                    info!("Process {:?} exited with exit code {}", child, ret_code);
                    State::Exited
                } else {
                    State::Running
                }
            }
            WaitStatus::Stopped(child, signal) => {
                ret = Some(StopReason {
                    reason: State::Stopped,
                    info: Info::Signalled(signal),
                });
                State::Stopped
            }
            WaitStatus::Signaled(pid, signal, has_coredump) => {
                ret = Some(StopReason {
                    reason: State::Terminated,
                    info: Info::Signalled(signal),
                });
                State::Terminated
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

            // For detach to work we need to be stopped! Hence the stop and wait before
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
