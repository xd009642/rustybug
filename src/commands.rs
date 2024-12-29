use std::path::PathBuf;
use std::str::FromStr;
use thiserror::Error;
use tracing::error;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ParseError {
    #[error("invalid command \"{0}\"")]
    InvalidCommand(String),
    #[error("invalid argument at {index} ({arg}): {msg}")]
    InvalidArgument {
        index: usize,
        arg: String,
        msg: String,
    },
    #[error("invalid location given {0}")]
    InvalidLocation(LocationError),
    #[error("invalid expression given {0}")]
    InvalidExpression(ExpressionError),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum LocationError {
    #[error("unknown source location")]
    UnknownSourceLocation,
    #[error("couldn't parse address")]
    CouldntParseAddress,
    #[error("couldn't parse address, invalid hexadecimal")]
    InvalidHexAddress,
    #[error("invalid line number")]
    InvalidLineNumber,
    #[error("invalid file name")]
    InvalidFileName,
    #[error("too many arguments for location: {0}")]
    TooManyArgs(usize),
    #[error("no location provided")]
    Empty,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ExpressionError {
    #[error("invalid expression")]
    InvalidExpression,
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
    Print(Expression),
    ListBreakpoints,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expression {
    Registers,
}

impl Command {
    pub fn store_in_history(&self) -> bool {
        !matches!(self, Self::Null | Self::Help | Self::Quit)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    Address(u64),
    Line { file: PathBuf, line: usize },
}

impl FromStr for Command {
    type Err = ParseError;

    fn from_str(command: &str) -> Result<Self, Self::Err> {
        match command {
            "q" | "quit" => Ok(Self::Quit),
            "logs" => Ok(Self::ToggleLogs),
            "?" | "help" => Ok(Self::Help),
            "continue" | "cont" | "c" => Ok(Self::Continue),
            "restart" => Ok(Self::Restart),
            "list" | "l" => Ok(Self::ListBreakpoints),
            x if x.starts_with("print ") => {
                let expr_str = x.trim_start_matches("print ");
                let expr = Expression::from_str(expr_str).map_err(ParseError::InvalidExpression)?;
                Ok(Self::Print(expr))
            }
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
            x if x.starts_with("break ") => {
                let location_str = x.trim_start_matches("break ");
                let location =
                    Location::from_str(location_str).map_err(ParseError::InvalidLocation)?;
                Ok(Self::Break(location))
            }
            x if !x.trim().is_empty() => Err(ParseError::InvalidCommand(x.to_string())),
            _ => Ok(Self::Null),
        }
    }
}

impl FromStr for Location {
    type Err = LocationError;

    fn from_str(location: &str) -> Result<Self, Self::Err> {
        let args = location.split_whitespace().collect::<Vec<&str>>();
        if args.len() == 1 {
            let addr = args[0];
            let addr = if addr.starts_with("0x") {
                let hex_addr = addr.strip_prefix("0x").unwrap();
                let addr = u64::from_str_radix(hex_addr, 16).map_err(|e| {
                    error!("Invalid hexadecimal: {}", e);
                    LocationError::InvalidHexAddress
                })?;
                addr
            } else {
                let addr = addr.parse::<u64>().map_err(|e| {
                    error!("Invalid integral address");
                    LocationError::CouldntParseAddress
                })?;
                addr
            };
            Ok(Location::Address(addr))
        } else if args.len() == 2 {
            let file = PathBuf::from(args[0]);
            let line = args[1].parse::<usize>().map_err(|e| {
                error!("Invalid line number: {}", e);
                LocationError::InvalidLineNumber
            })?;
            Ok(Location::Line { file, line })
        } else if args.is_empty() {
            Err(LocationError::Empty)
        } else {
            Err(LocationError::TooManyArgs(args.len()))
        }
    }
}

impl FromStr for Expression {
    type Err = ExpressionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "registers" {
            Ok(Expression::Registers)
        } else {
            Err(ExpressionError::InvalidExpression)
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
        assert_eq!(Command::from_str("logs").unwrap(), Command::ToggleLogs);
        assert_eq!(Command::from_str("l").unwrap(), Command::ListBreakpoints);
        assert_eq!(Command::from_str("help").unwrap(), Command::Help);
        assert_eq!(Command::from_str("?").unwrap(), Command::Help);
        assert_eq!(Command::from_str("restart").unwrap(), Command::Restart);
        assert_eq!(
            Command::from_str("load help.rs").unwrap(),
            Command::Load(PathBuf::from("help.rs"))
        );
        assert_eq!(
            Command::from_str("attach 546").unwrap(),
            Command::Attach(546)
        );
        assert_eq!(Command::from_str("continue").unwrap(), Command::Continue);
        assert_eq!(
            Command::from_str("print registers").unwrap(),
            Command::Print(Expression::Registers)
        );
        assert_eq!(Command::from_str("").unwrap(), Command::Null);
    }

    #[test]
    fn invalid_command_args() {
        assert!(matches!(
            Command::from_str("attach boop"),
            Err(ParseError::InvalidArgument { .. })
        ));
        assert_eq!(
            Command::from_str("dance"),
            Err(ParseError::InvalidCommand("dance".to_string()))
        );
        assert_eq!(
            Command::from_str("break main.rs"),
            Err(ParseError::InvalidLocation(
                LocationError::CouldntParseAddress
            ))
        );
        assert_eq!(
            Command::from_str("break 1 main.rs"),
            Err(ParseError::InvalidLocation(
                LocationError::InvalidLineNumber
            ))
        );
        assert_eq!(
            Command::from_str("break main.rs 1 2"),
            Err(ParseError::InvalidLocation(LocationError::TooManyArgs(3)))
        );
        assert_eq!(
            Command::from_str("break "),
            Err(ParseError::InvalidLocation(LocationError::Empty))
        );
        assert_eq!(
            Command::from_str("break 0xgg"),
            Err(ParseError::InvalidLocation(
                LocationError::InvalidHexAddress
            ))
        );
    }

    #[test]
    fn break_command_parsing() {
        let b = Command::from_str("break main.rs 5").unwrap();
        match b {
            Command::Break(location) => {
                if let Location::Line { file, line } = location {
                    assert_eq!(file, PathBuf::from("main.rs"));
                    assert_eq!(line, 5);
                } else {
                    panic!(
                        "Location misparsed. Should be a file line pair: {:?}",
                        location
                    );
                }
            }
            e => panic!("Invalid command parsed: {:?}", e),
        }

        let b = Command::from_str("break 0x12AD6").unwrap();
        match b {
            Command::Break(location) => {
                if let Location::Address(addr) = location {
                    assert_eq!(addr, 0x12ad6);
                } else {
                    panic!("Location misparsed. Should be an address: {:?}", location);
                }
            }
            e => panic!("Invalid command parsed: {:?}", e),
        }

        let b = Command::from_str("break 1234").unwrap();
        match b {
            Command::Break(location) => {
                if let Location::Address(addr) = location {
                    assert_eq!(addr, 1234);
                } else {
                    panic!("Location misparsed. Should be an address: {:?}", location);
                }
            }
            e => panic!("Invalid command parsed: {:?}", e),
        }
    }
}
