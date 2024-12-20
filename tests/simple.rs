//! In these tests we'll just run a program setting no breakpoints.
use rusty_fork::rusty_fork_test;
use rustybug::{Args, DebuggerStateMachine, State};

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
                input: test.into()
            };
            let mut sm = DebuggerStateMachine::start(args).unwrap();

            while State::Finished != sm.wait().unwrap() {

            }
        }
    }


}
