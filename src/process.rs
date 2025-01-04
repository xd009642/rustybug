use crate::breakpoint::*;
use crate::linux::launch_program;
use crate::ptrace_control::*;
use libc::{c_int, user_fpregs_struct, user_regs_struct};
use nix::errno::Errno;
use nix::sys::ptrace::{self, regset};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::*;
use nix::unistd::Pid;
use procfs::process::{MMapPath, Process as PfsProcess};
use std::os::fd::{AsRawFd, OwnedFd};
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
pub enum TrapType {
    SingleStep,
    SoftwareBreak,
    HardwareBreak,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Event {
    Exit,
    Exec,
    Fork,
    Vfork,
    Spawn,
}

impl TryFrom<i32> for Event {
    type Error = Errno;

    fn try_from(event: i32) -> Result<Self, Self::Error> {
        use nix::libc::*;
        // Hmm need a way to get PID into this if I want to use try_from and also report the new
        // spawned/forked children PIDs
        match event {
            PTRACE_EVENT_FORK => Ok(Self::Fork),
            PTRACE_EVENT_VFORK => Ok(Self::Vfork),
            PTRACE_EVENT_CLONE => Ok(Self::Spawn),
            PTRACE_EVENT_EXEC => Ok(Self::Exec),
            PTRACE_EVENT_EXIT => Ok(Self::Exit),
            _ => Err(Errno::UnknownErrno),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct StopReason {
    pub reason: State,
    pub info: Info,
    pub event: Option<Event>,
    pub trap_reason: Option<TrapType>,
}

impl StopReason {
    fn new(reason: State, info: Info) -> Self {
        Self {
            reason,
            info,
            event: None,
            trap_reason: None,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Info {
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
    #[error("couldn't use kill syscall on process")]
    KillFailed,
}

#[derive(Debug)]
pub struct Process {
    pid: Pid,
    stdout_reader: Option<OwnedFd>,
    pub addr_offset: u64,
    terminate_on_end: bool,
    state: State,
    breakpoints: Vec<Breakpoint>,
}

impl Process {
    pub fn launch(path: &Path) -> Result<Self, ProcessError> {
        let handle = launch_program(path)
            .map_err(|e| {
                error!("Failed to launch: {}", e);
                ProcessError::LaunchFailed
            })?
            .ok_or(ProcessError::NoPid)?;

        let pid = handle.pid;
        let stdout_reader = handle.stdout_reader;

        if stdout_reader.is_none() {
            info!("No handle to process stdout returned");
        }

        let addr_offset = get_addr_offset(pid);

        let mut ret = Self {
            pid,
            stdout_reader,
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
            stdout_reader: None,
            addr_offset,
            terminate_on_end: false,
            state: State::Stopped,
            breakpoints: vec![],
        };

        let timeout = Duration::from_secs(15);
        ret.blocking_wait_on_signal(timeout)?;

        Ok(ret)
    }

    pub fn stop(&self) -> Result<(), ProcessError> {
        kill(self.pid, Signal::SIGSTOP).map_err(|e| {
            error!("Couldn't stop process: {}", e);
            ProcessError::KillFailed
        })
    }

    pub fn pc(&self) -> Result<u64, ProcessError> {
        current_instruction_pointer(self.pid)
            .map(|x| x as u64)
            .map_err(|e| {
                error!("Couldn't read PC register: {}", e);
                ProcessError::RegisterReadFailed
            })
    }

    pub fn stop_on_events(&self) {
        if let Err(e) = trace_children(self.pid) {
            error!("Won't stop when a child forks/clones/execs: {}", e);
        }
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
                .jump_to(self.pid)
                .map_err(|_| ProcessError::ContinueFailed)?;
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
        info!(
            "Setting breakpoint at 0x{:x} (corrected 0x{:x})",
            addr,
            addr + self.addr_offset
        );
        let bp = Breakpoint::new(self.pid, addr + self.addr_offset).map_err(|e| {
            error!("Failed to set breakpoint: {}", e);
            ProcessError::BreakpointSetFailed
        })?;

        let id = bp.id;
        self.breakpoints.push(bp);
        Ok(id)
    }

    pub fn breakpoints(&self) -> &[Breakpoint] {
        self.breakpoints.as_slice()
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
                ret = Some(StopReason::new(State::Exited, Info::Return(ret_code as u8)));
                if child == self.pid {
                    info!("Process {:?} exited with exit code {}", child, ret_code);
                    self.pid = Pid::from_raw(0);
                    State::Exited
                } else {
                    State::Running
                }
            }
            WaitStatus::Stopped(child, signal) => {
                ret = Some(StopReason::new(State::Stopped, Info::Signalled(signal)));
                State::Stopped
            }
            WaitStatus::Signaled(pid, signal, has_coredump) => {
                ret = Some(StopReason::new(State::Terminated, Info::Signalled(signal)));
                State::Terminated
            }
            WaitStatus::PtraceEvent(pid, signal, event) => {
                let event = match Event::try_from(event) {
                    Ok(e) => Some(e),
                    Err(e) => {
                        error!("Failed to extract ptrace event from {}: {}", event, e);
                        None
                    }
                };
                let mut reason = StopReason::new(State::Stopped, Info::Signalled(signal));
                reason.event = event;
                ret = Some(reason);
                State::Stopped
            }
            sig => unimplemented!("{:?}", sig),
        };
        if let Some(ret) = ret.as_mut() {
            match ptrace::getsiginfo(self.pid) {
                Ok(sig_info) => {
                    pub const TRAP_TRACE: c_int = 2;
                    pub const TRAP_HWBKPT: c_int = 4;
                    pub const SI_KERNEL: c_int = 0x80;
                    ret.trap_reason = match sig_info.si_code {
                        TRAP_TRACE => Some(TrapType::SingleStep),
                        SI_KERNEL => Some(TrapType::SoftwareBreak),
                        TRAP_HWBKPT => Some(TrapType::HardwareBreak),
                        _ => None,
                    };
                }
                Err(e) => {
                    warn!("Couldn't get sig info: {}", e);
                }
            }
        }
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

    pub fn read_stdout(&mut self) -> Option<String> {
        let reader = self.stdout_reader.as_ref()?;
        let mut buf = [0u8; 1024];
        let len = unsafe { libc::read(reader.as_raw_fd(), std::mem::transmute(&mut buf), 1024) };
        if len > 0 {
            let string = String::from_utf8_lossy(&buf[..(len as usize)]).into_owned();
            Some(string)
        } else {
            None
        }
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
        if let Ok(auxv) = proc.auxv() {
            if let Some(entry) = auxv.get(&libc::AT_ENTRY) {
                return *entry;
            }
        }
        if let Ok(maps) = proc.maps() {
            println!("{:?}", maps);
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
