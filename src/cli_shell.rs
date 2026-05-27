use std::env;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliCommand {
    RunTui,
    PrintTopLevelHelp,
    PrintVersion,
    Gateway(GatewayShellCommand),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayShellCommand {
    PrintHelp,
    Runtime(GatewayRuntimeCommand),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayRuntimeCommand {
    Run,
    Start,
    Stop,
    Restart,
    Status,
    Logs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliParseError {
    UnknownCommand(String),
    UnknownOption(String),
    UnexpectedArgument { command: String, argument: String },
}

impl std::fmt::Display for CliParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCommand(command) => write!(formatter, "unknown command: {command}"),
            Self::UnknownOption(option) => write!(formatter, "unknown option: {option}"),
            Self::UnexpectedArgument { command, argument } => {
                write!(formatter, "unexpected argument for {command}: {argument}")
            }
        }
    }
}

pub fn parse_env_args() -> Result<CliCommand, CliParseError> {
    parse_args(env::args().skip(1))
}

pub fn parse_args<I, S>(args: I) -> Result<CliCommand, CliParseError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = args.into_iter().map(Into::into).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(CliCommand::PrintTopLevelHelp),
            "-V" | "--version" => return Ok(CliCommand::PrintVersion),
            "--dev" | "--demo" | "--mouse-capture" | "--no-mouse-capture" => {}
            "gateway" => return parse_gateway_args(args),
            value if value.starts_with('-') => {
                return Err(CliParseError::UnknownOption(value.to_string()));
            }
            value => return Err(CliParseError::UnknownCommand(value.to_string())),
        }
    }
    Ok(CliCommand::RunTui)
}

fn parse_gateway_args<I>(mut args: std::iter::Peekable<I>) -> Result<CliCommand, CliParseError>
where
    I: Iterator<Item = String>,
{
    let Some(arg) = args.next() else {
        return Ok(CliCommand::Gateway(GatewayShellCommand::PrintHelp));
    };
    match arg.as_str() {
        "-h" | "--help" => Ok(CliCommand::Gateway(GatewayShellCommand::PrintHelp)),
        "run" => {
            parse_gateway_runtime_args(GatewayRuntimeCommand::Run, "flyflor gateway run", args)
        }
        "start" => {
            parse_gateway_runtime_args(GatewayRuntimeCommand::Start, "flyflor gateway start", args)
        }
        "stop" => {
            parse_gateway_runtime_args(GatewayRuntimeCommand::Stop, "flyflor gateway stop", args)
        }
        "restart" => parse_gateway_runtime_args(
            GatewayRuntimeCommand::Restart,
            "flyflor gateway restart",
            args,
        ),
        "status" => parse_gateway_runtime_args(
            GatewayRuntimeCommand::Status,
            "flyflor gateway status",
            args,
        ),
        "logs" => {
            parse_gateway_runtime_args(GatewayRuntimeCommand::Logs, "flyflor gateway logs", args)
        }
        value if value.starts_with('-') => Err(CliParseError::UnknownOption(value.to_string())),
        value => Err(CliParseError::UnknownCommand(format!("gateway {value}"))),
    }
}

fn parse_gateway_runtime_args<I>(
    command: GatewayRuntimeCommand,
    command_name: &str,
    mut args: std::iter::Peekable<I>,
) -> Result<CliCommand, CliParseError>
where
    I: Iterator<Item = String>,
{
    if let Some(argument) = args.next() {
        if matches!(argument.as_str(), "-h" | "--help") {
            return Ok(CliCommand::Gateway(GatewayShellCommand::PrintHelp));
        }
        return Err(CliParseError::UnexpectedArgument {
            command: command_name.to_string(),
            argument,
        });
    }
    Ok(CliCommand::Gateway(GatewayShellCommand::Runtime(command)))
}

pub fn top_level_help() -> String {
    format!(
        "\
flyflor {version}

Usage:
  flyflor [OPTIONS]
  flyflor gateway [OPTIONS]

Commands:
  gateway              Show gateway runtime commands

Options:
  -h, --help           Print help
  -V, --version        Print version
      --dev            Start the TUI with internal dev diagnostics
      --demo           Start the TUI with demo data
      --mouse-capture  Force terminal mouse capture
      --no-mouse-capture
                       Disable terminal mouse capture

Environment:
  FLYFLOR_WS_URL       WebSocket URL for the existing Flyflor gateway
",
        version = env!("CARGO_PKG_VERSION")
    )
}

pub fn gateway_help() -> String {
    "\
flyflor gateway

Usage:
  flyflor gateway [OPTIONS]
  flyflor gateway <COMMAND>

Commands:
  run                  Run gateway runtime in the foreground
  start                Start gateway runtime daemon
  stop                 Stop gateway runtime daemon
  restart              Restart gateway runtime daemon
  status               Print gateway runtime status
  logs                 Print gateway runtime log tail

Options:
  -h, --help           Print gateway help

Note:
  Gateway runtime is a CLI-side /ws bridge. It does not modify the Flyflor
  kernel or write kernel databases.
"
    .to_string()
}

pub fn version_text() -> String {
    format!("flyflor {}\n", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_tui_without_args() {
        assert_eq!(
            parse_args(std::iter::empty::<&str>()),
            Ok(CliCommand::RunTui)
        );
    }

    #[test]
    fn tui_options_do_not_prevent_default_tui_launch() {
        assert_eq!(
            parse_args(["--dev", "--mouse-capture", "--demo"]),
            Ok(CliCommand::RunTui)
        );
    }

    #[test]
    fn parses_top_level_help_and_version() {
        assert_eq!(parse_args(["-h"]), Ok(CliCommand::PrintTopLevelHelp));
        assert_eq!(parse_args(["--version"]), Ok(CliCommand::PrintVersion));
    }

    #[test]
    fn parses_gateway_help() {
        assert_eq!(
            parse_args(["gateway", "-h"]),
            Ok(CliCommand::Gateway(GatewayShellCommand::PrintHelp))
        );
        assert_eq!(
            parse_args(["gateway"]),
            Ok(CliCommand::Gateway(GatewayShellCommand::PrintHelp))
        );
    }

    #[test]
    fn defines_gateway_runtime_commands_without_adapters() {
        assert_eq!(
            parse_args(["gateway", "run"]),
            Ok(CliCommand::Gateway(GatewayShellCommand::Runtime(
                GatewayRuntimeCommand::Run
            )))
        );
        assert_eq!(
            parse_args(["gateway", "start"]),
            Ok(CliCommand::Gateway(GatewayShellCommand::Runtime(
                GatewayRuntimeCommand::Start
            )))
        );
        assert_eq!(
            parse_args(["gateway", "status"]),
            Ok(CliCommand::Gateway(GatewayShellCommand::Runtime(
                GatewayRuntimeCommand::Status
            )))
        );
    }

    #[test]
    fn rejects_unknown_shell_input() {
        assert_eq!(
            parse_args(["--bad"]),
            Err(CliParseError::UnknownOption("--bad".to_string()))
        );
        assert_eq!(
            parse_args(["chat"]),
            Err(CliParseError::UnknownCommand("chat".to_string()))
        );
        assert_eq!(
            parse_args(["gateway", "run", "--port", "8787"]),
            Err(CliParseError::UnexpectedArgument {
                command: "flyflor gateway run".to_string(),
                argument: "--port".to_string()
            })
        );
    }

    #[test]
    fn help_mentions_required_entrypoints() {
        let help = top_level_help();
        assert!(help.contains("flyflor [OPTIONS]"));
        assert!(help.contains("flyflor gateway [OPTIONS]"));
        assert!(gateway_help().contains("flyflor gateway <COMMAND>"));
    }
}
