use clap::Parser;
use std::path::PathBuf;

pub mod breakpoint;
//pub mod test_loader;
pub mod linux;
pub mod ptrace_control;

/// rustybug a moderately simple debugger written in rust. Not intended to be feature complete more
/// a toy project and way to test some tarpaulin assumptions.
#[derive(Debug, Parser)]
pub struct Args {
    /// Executable to debug
    input: PathBuf,
}

fn main() {
    let args = Args::parse();
}
