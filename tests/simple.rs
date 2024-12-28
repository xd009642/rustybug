//! In these tests we'll just run a program setting no breakpoints.
use rusty_fork::rusty_fork_test;
use rustybug::{Args, DebuggerStateMachine, State};
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
}
