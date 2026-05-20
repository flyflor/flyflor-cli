use std::{
    env,
    fmt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KernelMode {
    Mock,
    ManagedLocalBinary,
    RemoteWs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BinarySource {
    CliArgument,
    EnvOverride,
    InstalledDefault,
    DevFallback,
}

#[derive(Clone, Debug)]
pub struct BinaryResolution {
    pub requested_path: Option<PathBuf>,
    pub resolved_path: PathBuf,
    pub source: BinarySource,
    pub attempted_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct CliConfig {
    pub kernel_mode: KernelMode,
    pub binary_path: Option<PathBuf>,
    pub ws_url: Option<String>,
    pub host: String,
    pub port: u16,
    pub dev_mode: bool,
    pub mouse_capture: Option<bool>,
    pub user_id: String,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct KernelProcessConfig {
    pub binary_path: PathBuf,
    pub args: Vec<String>,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug)]
pub enum ProcessStatus {
    Starting,
    Running,
    Exited,
    NotStarted,
}

#[derive(Clone, Debug)]
pub struct KernelProcessState {
    pub status: ProcessStatus,
    pub pid: Option<u32>,
    pub started_at: Option<Instant>,
    pub last_exit_code: Option<i32>,
    pub stderr_tail: Vec<String>,
}

pub struct ManagedKernel {
    child: Child,
    pub state: KernelProcessState,
}

#[derive(Debug)]
pub struct CliError {
    pub message: String,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CliError {}

impl CliConfig {
    pub fn from_env() -> Result<Self, CliError> {
        let args: Vec<String> = env::args().collect();
        if args.iter().any(|arg| arg == "-h" || arg == "--help") {
            print_help();
            std::process::exit(0);
        }
        let mut ws_url = None;
        let mut binary_path = None;
        let mut host = String::from("127.0.0.1");
        let mut port = 8787;
        let mut dev_mode = false;
        let mut mouse_capture = None;
        let mut user_id = String::from("rust-cli-user");
        let mut display_name = None;
        let mut mock = false;

        let mut index = 1;
        while index < args.len() {
            match args[index].as_str() {
                "--ws-url" => {
                    index += 1;
                    ws_url = args.get(index).cloned();
                }
                "--binary" => {
                    index += 1;
                    binary_path = args.get(index).map(PathBuf::from);
                }
                "--host" => {
                    index += 1;
                    if let Some(value) = args.get(index) {
                        host = value.clone();
                    }
                }
                "--port" => {
                    index += 1;
                    if let Some(value) = args.get(index) {
                        port = value.parse().map_err(|_| CliError {
                            message: format!("invalid port: {value}"),
                        })?;
                    }
                }
                "--dev" => dev_mode = true,
                "--mouse-capture" => mouse_capture = Some(true),
                "--no-mouse-capture" => mouse_capture = Some(false),
                "--user-id" => {
                    index += 1;
                    if let Some(value) = args.get(index) {
                        user_id = value.clone();
                    }
                }
                "--display-name" => {
                    index += 1;
                    display_name = args.get(index).cloned();
                }
                "--mock" => mock = true,
                _ => {}
            }
            index += 1;
        }

        if !dev_mode {
            dev_mode = env::var("FLYFLOR_DEV")
                .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
                .unwrap_or(false);
        }

        if mouse_capture.is_none() {
            mouse_capture = match env::var("FLYFLOR_MOUSE_CAPTURE") {
                Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "on" | "ON") => Some(true),
                Ok(value) if matches!(value.as_str(), "0" | "false" | "FALSE" | "off" | "OFF") => Some(false),
                _ => None,
            };
        }

        let kernel_mode = if mock {
            KernelMode::Mock
        } else if ws_url.is_some() {
            KernelMode::RemoteWs
        } else {
            KernelMode::ManagedLocalBinary
        };

        Ok(Self {
            kernel_mode,
            binary_path,
            ws_url,
            host,
            port,
            dev_mode,
            mouse_capture,
            user_id,
            display_name,
        })
    }
}

fn print_help() {
    println!(
        "\
flyflor

Usage:
  flyflor [--mock]
  flyflor [--binary <path>] [--host <host>] [--port <port>]
  flyflor [--ws-url <ws-url>]

Options:
  -h, --help              Show this help
  --mock                  Run static mock TUI
  --binary <path>         Explicit Flyflor binary path
  --ws-url <url>          Connect to an existing Flyflor /ws endpoint
  --host <host>           Managed local host (default: 127.0.0.1)
  --port <port>           Managed local port (default: 8787)
  --dev                   Enable dev overlay
  --mouse-capture         Force mouse capture on
  --no-mouse-capture      Force mouse capture off
  --user-id <id>          User id for gateway.message.send
  --display-name <name>   Optional display name for gateway.message.send
"
    );
}

pub fn resolve_binary(config: &CliConfig) -> Result<BinaryResolution, CliError> {
    if let Some(ws_url) = &config.ws_url {
        if !ws_url.is_empty() {
            return Err(CliError {
                message: "binary resolution skipped for remote ws mode".to_string(),
            });
        }
    }

    let mut attempted = Vec::new();

    if let Some(path) = &config.binary_path {
        attempted.push(path.clone());
        if path.exists() {
            return Ok(BinaryResolution {
                requested_path: Some(path.clone()),
                resolved_path: path.clone(),
                source: BinarySource::CliArgument,
                attempted_paths: attempted,
            });
        }
    }

    if let Ok(value) = env::var("FLYFLOR_BINARY") {
        let path = PathBuf::from(value);
        attempted.push(path.clone());
        if path.exists() {
            return Ok(BinaryResolution {
                requested_path: None,
                resolved_path: path,
                source: BinarySource::EnvOverride,
                attempted_paths: attempted,
            });
        }
    }

    let home = env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("~"));
    let installed = home.join(".flyflor").join("dist").join("flyflor");
    attempted.push(installed.clone());
    if installed.exists() {
        return Ok(BinaryResolution {
            requested_path: None,
            resolved_path: installed,
            source: BinarySource::InstalledDefault,
            attempted_paths: attempted,
        });
    }

    let dev = env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("flyflor")
        .join("dist")
        .join("flyflor");
    attempted.push(dev.clone());
    if dev.exists() {
        return Ok(BinaryResolution {
            requested_path: None,
            resolved_path: dev,
            source: BinarySource::DevFallback,
            attempted_paths: attempted,
        });
    }

    Err(CliError {
        message: format!(
            "unable to locate flyflor binary; attempted: {}",
            attempted
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}

pub fn managed_ws_url(config: &CliConfig) -> String {
    format!("ws://{}:{}/ws", config.host, config.port)
}

pub fn gateway_health_url(config: &CliConfig) -> String {
    format!("http://{}:{}/health", config.host, config.port)
}

pub fn spawn_kernel(config: &KernelProcessConfig) -> Result<ManagedKernel, CliError> {
    let mut command = Command::new(&config.binary_path);
    command
        .args(&config.args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let child = command.spawn().map_err(|error| CliError {
        message: format!(
            "failed to start flyflor binary {}: {error}",
            config.binary_path.display()
        ),
    })?;

    let pid = Some(child.id());
    Ok(ManagedKernel {
        child,
        state: KernelProcessState {
            status: ProcessStatus::Starting,
            pid,
            started_at: Some(Instant::now()),
            last_exit_code: None,
            stderr_tail: Vec::new(),
        },
    })
}

pub fn wait_for_health(url: &str, timeout: Duration) -> Result<(), CliError> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        let response = ureq::get(url).call();
        if let Ok(response) = response {
            if response.status() == 200 {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err(CliError {
        message: format!("gateway health check timed out: {url}"),
    })
}

impl ManagedKernel {
    pub fn mark_running(&mut self) {
        self.state.status = ProcessStatus::Running;
    }

    pub fn poll_exit(&mut self) {
        if let Ok(Some(status)) = self.child.try_wait() {
            self.state.status = ProcessStatus::Exited;
            self.state.last_exit_code = status.code();
        }
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.state.status = ProcessStatus::Exited;
    }
}

pub fn path_label(path: &Path) -> String {
    path.display().to_string()
}
