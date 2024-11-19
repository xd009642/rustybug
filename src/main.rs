use clap::Parser;
use rustybug::{Args, DebuggerStateMachine, State};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

fn main() -> anyhow::Result<()> {
    let fmt_layer = fmt::layer();
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();
    tracing::info!("rustybug");

    let args = Args::parse();

    let mut sm = DebuggerStateMachine::start(args)?;

    while State::Finished != sm.wait()? {}

    Ok(())
}
