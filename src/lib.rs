use crate::commands::Location;
use crate::elf::ExecutableFile;
use crate::process::{Process, Registers, StopReason};
use anyhow::Context;
use clap::Parser;
use nix::unistd::Pid;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, info, warn};

pub use crate::process::State;

pub mod breakpoint;
pub mod commands;
pub mod elf;
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
    elf: Option<ExecutableFile>,
    args: Args,
}

impl DebuggerStateMachine {
    pub fn start(args: Args) -> anyhow::Result<Self> {
        let (mut root, elf) = if let Some(input) = args.input.as_ref() {
            let elf = match ExecutableFile::load(input) {
                Ok(elf) => Some(elf),
                Err(e) => {
                    warn!("Failed to load elf file: {}", e);
                    None
                }
            };
            (Process::launch(input)?, elf)
        } else if let Some(pid) = args.pid {
            let pid = Pid::from_raw(pid);
            (Process::attach(pid)?, None)
        } else {
            panic!("You should provide an executable name or PID");
        };

        info!(pid=?root.pid(), "program launch.");

        let old = root.addr_offset;
        root.addr_offset = root.addr_offset - elf.as_ref().map(|x| x.entry_address()).unwrap_or_default();
        info!("Used entry addr to correct offset from {} to {}", old, root.addr_offset);

        debug!(process=?root);

        Ok(Self { root, elf, args })
    }

    pub fn blocking_wait(&mut self, duration: Duration) -> anyhow::Result<StopReason> {
        Ok(self.root.blocking_wait_on_signal(duration)?)
    }

    pub fn wait(&mut self) -> anyhow::Result<Option<StopReason>> {
        Ok(self.root.wait_on_signal()?)
    }

    pub fn cont(&mut self) -> anyhow::Result<()> {
        if self.root.state() == State::Stopped {
            self.root.resume()?;
        }
        Ok(())
    }

    pub fn step(&mut self) -> anyhow::Result<()> {
        if self.root.state() == State::Stopped {
            self.root.step()?;
        }
        Ok(())
    }

    pub fn get_registers(&self) -> anyhow::Result<Registers> {
        if self.root.state() != State::Stopped {
            anyhow::bail!(
                "Process must be stopped to read registers: {:?}",
                self.root.state()
            );
        }
        let regs = self.root.get_all_registers()?;
        Ok(regs)
    }

    pub fn set_break(&mut self, location: &Location) -> anyhow::Result<u64> {
        match location {
            Location::Address(addr) => {
                let id = self.root.set_breakpoint(*addr)?;
                Ok(id)
            }
            Location::Line { .. } => {
                anyhow::bail!("Need to implement file+line breakpoint setting")
            }
            Location::Function(fn_name) => {
                if let Some(elf) = self.elf.as_ref() {
                    let functions = elf.find_functions(&fn_name)?;

                    for (unit, offset) in &functions {
                        let die = unit.entry(*offset)?;
                        let low_pc = die.attr_value(gimli::DW_AT_low_pc);
                        let low_pc = match low_pc {
                            Ok(Some(gimli::AttributeValue::Addr(x))) => x,
                            _ => continue,
                        };
                        let id = self.root.set_breakpoint(low_pc)?;
                        return Ok(id);
                    }
                    anyhow::bail!("No function found we could attach a breakpoint to");
                } else {
                    anyhow::bail!("No elf file loaded");
                }
            }
        }
    }

    pub fn list_breakpoints(&self) {
        info!("Breakpoints: {:?}", self.root.breakpoints());
    }

    pub fn log_status(&self) {
        let state = self.root.state();
        if state == State::Stopped {
            if let Ok(addr) = self.root.pc() {
                info!("Root process is stopped at {:x}", addr);
            } else {
                info!("Root process is stopped at an unknown place");
            }
        } else {
            info!("Root process is {:?}", state);
        }
    }

    pub fn root_process(&self) -> &Process {
        &self.root
    }

    pub fn root_process_mut(&mut self) -> &mut Process {
        &mut self.root
    }

    pub fn has_elf_file(&self) -> bool {
        self.elf.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_on_invalid_launch() {
        let args = Args {
            input: Some("i-am-not-a-real-program-you-cannot-run-me".into()),
            pid: None,
        };
        let sm = DebuggerStateMachine::start(args);
        assert!(sm.is_err());
    }

    #[test]
    #[should_panic]
    fn panic_if_starting_nothing() {
        let _ = DebuggerStateMachine::start(Args::default());
    }
}
