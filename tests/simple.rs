//! In these tests we'll just run a program setting no breakpoints.
use nix::sys::signal::Signal;
use rusty_fork::rusty_fork_test;
use rustybug::commands::Location;
use rustybug::{
    process::{Event, Info, Process, ProcessError, TrapType},
    Args, DebuggerStateMachine, State,
};
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tracing_test::traced_test;

const TESTS: &[&'static str] = &[
    "tests/data/apps/build/test_project",
    "tests/data/apps/build/threads",
    //"tests/data/apps/build/user_signal"
];

rusty_fork_test! {
    #[test]
    #[traced_test]
    fn no_breakpoints() {
        for test in TESTS {
            println!("Running: {}", test);
            let args = Args {
                input: Some(test.into()),
                pid: None,
            };
            let mut sm = DebuggerStateMachine::start(args).unwrap();

            assert!(sm.has_elf_file());

            sm.cont().unwrap();

            while !matches!(sm.wait().unwrap().map(|x| x.reason), Some(State::Exited)) {

            }

            assert!(sm.wait().is_err());
        }
    }

    #[test]
    #[traced_test]
    fn attach_doesnt_sigkill() {

        let mut child = Command::new("tests/data/apps/build/dont_stop")
            .spawn()
            .unwrap();

        let pid = child.id() as i32;

        let args = Args {
            input: None,
            pid: Some(pid),
        };

        let mut sm = DebuggerStateMachine::start(args).unwrap();

        sm.cont().unwrap();
        sm.wait().unwrap();
        sm.wait().unwrap();

        std::mem::drop(sm);

        // An eternity for an OS
        std::thread::sleep(Duration::from_secs(1));

        assert!(child.try_wait().unwrap().is_none());

        let _ = child.kill();
    }

    #[test]
    #[traced_test]
    fn register_peek_poke() {
            let mut proc = Process::launch(Path::new("tests/data/apps/build/test_project")).unwrap();

            let mut regs = proc.get_all_registers().unwrap();

            let expected_rax = regs.regs.rax.overflowing_add(42).0;
            regs.regs.rax = expected_rax;

            let expected_st0 = regs.fpregs.st_space[0].overflowing_add(56).0;
            regs.fpregs.st_space[0] = expected_st0;

            proc.write_all_registers(regs).unwrap();

            let actual_regs = proc.get_all_registers().unwrap();

            assert_eq!(actual_regs.regs.rax, expected_rax);
            assert_eq!(actual_regs.fpregs.st_space[0], expected_st0);

    }

    #[test]
    #[traced_test]
    fn continue_on_entry_breakpoint() {
        let mut proc = Process::launch(Path::new("tests/data/apps/build/test_project")).unwrap();
        proc.set_breakpoint(proc.pc().unwrap()).unwrap();
        proc.resume().unwrap();
        while Some(State::Exited) != proc.wait_on_signal().unwrap().map(|x| x.reason) {

        }
    }

    #[test]
    #[traced_test]
    fn step_to_end_works() {
        let mut proc = Process::launch(Path::new("tests/data/apps/build/test_project")).unwrap();

        let pc = proc.pc().unwrap();
        proc.step().unwrap();
        proc.blocking_wait_on_signal(Duration::from_secs(1)).unwrap();
        let new_pc = proc.pc().unwrap();
        assert!(new_pc > pc);

        proc.resume().unwrap();

        let stop_reason = proc.blocking_wait_on_signal(Duration::from_secs(1)).unwrap();
        assert_eq!(stop_reason.info, Info::Return(0));
        assert_eq!(stop_reason.reason, State::Exited);
    }

    #[test]
    #[traced_test]
    fn stop_on_events() {
        let mut proc = Process::launch(Path::new("tests/data/apps/build/test_project")).unwrap();

        proc.stop_on_events();
        proc.resume().unwrap();

        let reason =  proc.blocking_wait_on_signal(Duration::from_secs(2)).unwrap();

        assert_eq!(reason.event, Some(Event::Exit));

        proc.resume().unwrap();

        let reason =  proc.blocking_wait_on_signal(Duration::from_secs(1)).unwrap();
        assert_eq!(reason.event, None);
        assert_eq!(reason.reason, State::Exited);
        assert_eq!(reason.info, Info::Return(0));
    }

    #[test]
    #[traced_test]
    fn can_stop_process() {

        let mut proc = Process::launch(Path::new("tests/data/apps/build/dont_stop")).unwrap();

        proc.resume().unwrap();

        assert_eq!(proc.blocking_wait_on_signal(Duration::from_secs(1)), Err(ProcessError::Timeout));

        proc.stop().unwrap();

        assert!(proc.blocking_wait_on_signal(Duration::from_secs(1)).is_ok());
    }

    #[test]
    #[traced_test]
    fn breakpoint_on_function() {
        let args = Args {
            input: Some("tests/data/apps/build/test_project".into()),
            pid: None,
        };
        let mut sm = DebuggerStateMachine::start(args).unwrap();

        sm.set_break(&Location::Function("main".to_string())).unwrap();

        sm.cont();

        let reason = sm.blocking_wait(Duration::from_secs(5)).unwrap();

        assert_eq!(reason.trap_reason, Some(TrapType::SoftwareBreak));
        assert_eq!(reason.info, Info::Signalled(Signal::SIGTRAP));
        assert_eq!(reason.reason, State::Stopped);
        assert_eq!(reason.event, None);
    }
}
