//! In these tests we'll just run a program setting no breakpoints.
use rusty_fork::rusty_fork_test;
use rustybug::{process::Process, Args, DebuggerStateMachine, State};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const TESTS: &[&'static str] = &[
    "tests/data/apps/build/test_project",
    "tests/data/apps/build/threads",
    //"tests/data/apps/build/user_signal"
];

rusty_fork_test! {
    #[test]
    fn no_breakpoints() {
        for test in TESTS {
            println!("Running: {}", test);
            let args = Args {
                input: Some(test.into()),
                pid: None,
            };
            let mut sm = DebuggerStateMachine::start(args).unwrap();

            sm.cont().unwrap();

            while State::Exited != sm.wait().unwrap() {

            }
        }
    }

    #[test]
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
}
