use crate::breakpoint::*;
use crate::linux::launch_program;
use crate::ptrace_control::*;
use libc::{user_fpregs_struct, user_regs_struct};
use nix::sys::ptrace::{self, regset};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::*;
use nix::unistd::Pid;
use procfs::process::{MMapPath, Process as PfsProcess};
use std::path::Path;
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{error, info, warn};

#[derive(Clone, Debug)]
pub struct Registers {
    pub regs: user_regs_struct,
    pub fpregs: user_fpregs_struct,
}

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
    #[error("failed to step forwards")]
    SingleStepFailed,
    #[error("blocking operation timed out")]
    Timeout,
    #[error("failed to write data")]
    WriteFailed,
    #[error("couldn't read user registers")]
    RegisterReadFailed,
    #[error("couldn't read user fp registers")]
    FpRegisterReadFailed,
    #[error("couldn't write user registers")]
    RegisterWriteFailed,
    #[error("couldn't write user fp registers")]
    FpRegisterWriteFailed,
    #[error("couldn't add breakpoint")]
    BreakpointSetFailed,
}

#[derive(Debug)]
pub struct Process {
    pid: Pid,
    addr_offset: u64,
    terminate_on_end: bool,
    state: State,
    breakpoints: Vec<Breakpoint>,
}

impl Process {
    pub fn launch(path: &Path) -> Result<Self, ProcessError> {
        let pid = launch_program(path)
            .map_err(|e| {
                error!("Failed to launch: {}", e);
                ProcessError::LaunchFailed
            })?
            .ok_or(ProcessError::NoPid)?;

        let addr_offset = get_addr_offset(pid);

        let mut ret = Self {
            pid,
            addr_offset,
            terminate_on_end: true,
            state: State::Stopped,
            breakpoints: vec![],
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

        let addr_offset = get_addr_offset(pid);

        let mut ret = Self {
            pid,
            addr_offset,
            terminate_on_end: false,
            state: State::Stopped,
            breakpoints: vec![],
        };

        let timeout = Duration::from_secs(15);
        ret.blocking_wait_on_signal(timeout)?;

        Ok(ret)
    }

    pub fn resume(&mut self) -> Result<(), ProcessError> {
        info!(pid=%self.pid, "Continuing process");
        let mut bps: Vec<&mut Breakpoint> = self
            .breakpoints
            .iter_mut()
            .filter(|bp| bp.has_hit(self.pid).unwrap_or_default())
            .collect();
        if bps.len() > 1 {
            error!("breakpoint clashes: {:?}", bps);
        }
        if bps.is_empty() {
            continue_exec(self.pid, None).map_err(|_| ProcessError::ContinueFailed)?;
        } else {
            bps[0]
                .process(self.pid, true)
                .map_err(|_| ProcessError::ContinueFailed)?;
            continue_exec(self.pid, None).map_err(|_| ProcessError::ContinueFailed)?;
        }
        self.state = State::Running;
        Ok(())
    }

    pub fn step(&mut self) -> Result<(), ProcessError> {
        let mut bps: Vec<&mut Breakpoint> = self
            .breakpoints
            .iter_mut()
            .filter(|bp| bp.has_hit(self.pid).unwrap_or_default())
            .collect();
        if bps.len() > 1 {
            error!("breakpoint clashes: {:?}", bps);
        }
        if bps.is_empty() {
            single_step(self.pid).map_err(|_| ProcessError::SingleStepFailed)?;
        } else {
            bps[0]
                .process(self.pid, true)
                .map_err(|_| ProcessError::ContinueFailed)?;
            single_step(self.pid).map_err(|_| ProcessError::SingleStepFailed)?;
        }
        self.state = State::Stopped;
        Ok(())
    }

    pub fn pid(&self) -> Pid {
        self.pid
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn set_breakpoint(&mut self, addr: u64) -> Result<u64, ProcessError> {
        let bp = Breakpoint::new(self.pid, addr + self.addr_offset).map_err(|e| {
            error!("Failed to set breakpoint: {}", e);
            ProcessError::BreakpointSetFailed
        })?;

        let id = bp.id;
        self.breakpoints.push(bp);
        Ok(id)
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
            WaitStatus::StillAlive => self.state,
            sig @ WaitStatus::Exited(child, ret_code) => {
                ret = Some(StopReason {
                    reason: State::Exited,
                    info: Info::Return(ret_code as u8),
                });
                if child == self.pid {
                    info!("Process {:?} exited with exit code {}", child, ret_code);
                    self.pid = Pid::from_raw(0);
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

    pub fn write_user_area(&self, offset: u64, data: i64) -> Result<(), ProcessError> {
        write_to_address(self.pid, offset, data).map_err(|e| {
            error!("Failed to write to register offset({}): {}", offset, e);
            ProcessError::WriteFailed
        })
    }

    pub fn get_all_registers(&self) -> Result<Registers, ProcessError> {
        let regs = ptrace::getregs(self.pid).map_err(|e| {
            error!("Failed to read registers: {}", e);
            ProcessError::RegisterReadFailed
        })?;

        let fpregs = ptrace::getregset::<regset::NT_PRFPREG>(self.pid).map_err(|e| {
            error!("Failed to read fp registers: {}", e);
            ProcessError::FpRegisterReadFailed
        })?;

        // In the book they do the debug registers but they aren't in the nix crate so I'll save
        // them for now (maybe PR nix crate at some point or raise an issue when I understand them
        // more).

        Ok(Registers { regs, fpregs })
    }

    pub fn write_all_registers(&mut self, registers: Registers) -> Result<(), ProcessError> {
        self.write_gp_registers(registers.regs)?;
        self.write_fp_registers(registers.fpregs)
    }

    pub fn write_gp_registers(&mut self, regs: user_regs_struct) -> Result<(), ProcessError> {
        ptrace::setregs(self.pid, regs).map_err(|e| {
            error!("Failed to write registers: {}", e);
            ProcessError::RegisterWriteFailed
        })
    }

    pub fn write_fp_registers(&mut self, fpregs: user_fpregs_struct) -> Result<(), ProcessError> {
        ptrace::setregset::<regset::NT_PRFPREG>(self.pid, fpregs).map_err(|e| {
            error!("Failed to write fp registers: {}", e);
            ProcessError::FpRegisterWriteFailed
        })
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

fn get_addr_offset(pid: Pid) -> u64 {
    if let Ok(proc) = PfsProcess::new(pid.as_raw()) {
        let exe = proc.exe().ok();
        if let Ok(maps) = proc.maps() {
            let offset_info = maps.iter().find(|x| match (&x.pathname, exe.as_ref()) {
                (MMapPath::Path(p), Some(e)) => p == e,
                (MMapPath::Path(_), None) => true,
                _ => false,
            });
            if let Some(first) = offset_info {
                first.address.0
            } else {
                0
            }
        } else {
            0
        }
    } else {
        0
    }
}
