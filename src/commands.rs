use std::path::PathBuf;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid command \"{0}\"")]
    InvalidCommand(String),
    #[error("invalid argument at {index} ({arg}): {msg}")]
    InvalidArgument {
        index: usize,
        arg: String,
        msg: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Quit,
    ToggleLogs,
    Help,
    Restart,
    Load(PathBuf),
    Attach(i32),
    Continue,
    Break(Location),
    Null,
}

impl Command {
    pub fn store_in_history(&self) -> bool {
        !matches!(self, Self::Null | Self::Help | Self::Quit)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Location {
    Address(u64),
}

impl FromStr for Command {
    type Err = ParseError;

    fn from_str(command: &str) -> Result<Self, Self::Err> {
        match command {
            "q" | "quit" => Ok(Self::Quit),
            "l" | "logs" => Ok(Self::ToggleLogs),
            "?" | "help" => Ok(Self::Help),
            "restart" => Ok(Self::Restart),
            x if x.starts_with("load ") => {
                let path = x.trim_start_matches("load ");
                let path = PathBuf::from(path);
                Ok(Self::Load(path))
            }
            x if x.starts_with("attach ") => {
                let pid_str = x.trim_start_matches("attach ");
                let pid = pid_str.parse::<i32>();
                match pid {
                    Ok(pid) => Ok(Self::Attach(pid)),
                    Err(e) => Err(ParseError::InvalidArgument {
                        index: 0,
                        arg: pid_str.to_string(),
                        msg: e.to_string(),
                    }),
                }
            }
            x if !x.trim().is_empty() => Err(ParseError::InvalidCommand(x.to_string())),
            _ => Ok(Self::Null),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_parsing() {
        assert_eq!(Command::from_str("quit").unwrap(), Command::Quit);
        assert_eq!(Command::from_str("q").unwrap(), Command::Quit);
        assert_eq!(
            Command::from_str("load help.rs").unwrap(),
            Command::Load(PathBuf::from("help.rs"))
        );
        assert_eq!(
            Command::from_str("attach 546").unwrap(),
            Command::Attach(546)
        );
    }
}
