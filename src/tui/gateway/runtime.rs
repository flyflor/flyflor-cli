use std::{
    env,
    fs::{self, File, OpenOptions, create_dir_all},
    io::{self, Write},
    net::TcpStream,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tungstenite::{Error as WsError, Message, connect, stream::MaybeTlsStream};

use crate::{
    DEFAULT_WS_URL,
    kernel::{client::GatewayClientBootstrap, envelope::EnvelopeFactory},
    tui::gateway::channels::spawn_gateway_channel_runtime,
};

const CLI_HOME_ENV: &str = "FLYFLOR_CLI_HOME";
const FOREGROUND_ENV: &str = "FLYFLOR_GATEWAY_RUNTIME_FOREGROUND";
const STOP_WAIT: Duration = Duration::from_secs(5);
const STATUS_WAIT: Duration = Duration::from_secs(2);
const SOCKET_READ_TIMEOUT: Duration = Duration::from_millis(50);

pub fn should_run_foreground_from_env() -> bool {
    env::var(FOREGROUND_ENV)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
        .unwrap_or(false)
}

pub fn run_foreground() -> io::Result<()> {
    GatewayRuntime::from_env().run_foreground()
}

pub fn start() -> io::Result<GatewayStartReport> {
    GatewayRuntime::from_env().start()
}

pub fn stop() -> io::Result<GatewayStopReport> {
    GatewayRuntime::from_env().stop()
}

pub fn restart() -> io::Result<GatewayStartReport> {
    GatewayRuntime::from_env().restart()
}

pub fn status() -> io::Result<GatewayStatusReport> {
    GatewayRuntime::from_env().status()
}

pub fn logs(max_bytes: usize) -> io::Result<String> {
    GatewayRuntime::from_env().logs(max_bytes)
}

#[derive(Clone, Debug)]
pub struct GatewayRuntime {
    paths: GatewayRuntimePaths,
}

impl GatewayRuntime {
    pub fn from_env() -> Self {
        Self {
            paths: GatewayRuntimePaths::from_env(),
        }
    }

    pub fn run_foreground(&self) -> io::Result<()> {
        run_foreground_with_paths(self.paths.clone())
    }

    pub fn start(&self) -> io::Result<GatewayStartReport> {
        start_daemon(&self.paths)
    }

    pub fn stop(&self) -> io::Result<GatewayStopReport> {
        stop_daemon(&self.paths)
    }

    pub fn restart(&self) -> io::Result<GatewayStartReport> {
        self.stop()?;
        self.start()
    }

    pub fn status(&self) -> io::Result<GatewayStatusReport> {
        status_report(&self.paths)
    }

    pub fn logs(&self, max_bytes: usize) -> io::Result<String> {
        read_log_tail(&self.paths, max_bytes)
    }
}

#[derive(Clone, Debug)]
pub struct GatewayRuntimePathReport {
    pub home: PathBuf,
    pub state_dir: PathBuf,
    pub log_dir: PathBuf,
    pub pid_file: PathBuf,
    pub lock_file: PathBuf,
    pub stop_file: PathBuf,
    pub status_file: PathBuf,
    pub log_file: PathBuf,
}

impl GatewayRuntimePathReport {
    fn from_paths(paths: &GatewayRuntimePaths) -> Self {
        Self {
            home: paths.home.clone(),
            state_dir: paths.state_dir.clone(),
            log_dir: paths.log_dir.clone(),
            pid_file: paths.pid_file.clone(),
            lock_file: paths.lock_file.clone(),
            stop_file: paths.stop_file.clone(),
            status_file: paths.status_file.clone(),
            log_file: paths.log_file.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayRuntimeState {
    Running,
    Stopping,
    Stopped,
    Stale,
}

#[derive(Clone, Debug)]
pub struct GatewayStartReport {
    pub pid: u32,
    pub already_running: bool,
    pub paths: GatewayRuntimePathReport,
}

#[derive(Clone, Debug)]
pub struct GatewayStopReport {
    pub pid: Option<u32>,
    pub state: GatewayRuntimeState,
    pub paths: GatewayRuntimePathReport,
}

#[derive(Clone, Debug)]
pub struct GatewayStatusReport {
    pub pid: Option<u32>,
    pub state: GatewayRuntimeState,
    pub status_json: Option<String>,
    pub paths: GatewayRuntimePathReport,
}

#[derive(Clone, Debug)]
struct GatewayRuntimePaths {
    home: PathBuf,
    state_dir: PathBuf,
    log_dir: PathBuf,
    pid_file: PathBuf,
    lock_file: PathBuf,
    stop_file: PathBuf,
    status_file: PathBuf,
    log_file: PathBuf,
}

impl GatewayRuntimePaths {
    fn from_env() -> Self {
        let home = env::var(CLI_HOME_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(".flyflor-cli"));
        let state_dir = home.join("gateway");
        let log_dir = home.join("logs");
        Self {
            pid_file: state_dir.join("gateway.pid"),
            lock_file: state_dir.join("gateway.lock"),
            stop_file: state_dir.join("gateway.stop"),
            status_file: state_dir.join("status.json"),
            log_file: log_dir.join("gateway.log"),
            home,
            state_dir,
            log_dir,
        }
    }

    fn ensure(&self) -> io::Result<()> {
        create_dir_all(&self.state_dir)?;
        create_dir_all(&self.log_dir)?;
        Ok(())
    }
}

struct GatewayRuntimeGuard {
    paths: GatewayRuntimePaths,
    _lock: File,
    pid: u32,
}

impl GatewayRuntimeGuard {
    fn acquire(paths: GatewayRuntimePaths) -> io::Result<Self> {
        paths.ensure()?;
        remove_stale_runtime_files(&paths)?;
        let lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&paths.lock_file)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!(
                            "gateway runtime lock exists at {}; inspect gateway runtime status",
                            paths.lock_file.display()
                        ),
                    )
                } else {
                    error
                }
            })?;
        let pid = std::process::id();
        fs::write(&paths.pid_file, format!("{pid}\n"))?;
        let _ = fs::remove_file(&paths.stop_file);
        Ok(Self {
            paths,
            _lock: lock,
            pid,
        })
    }
}

impl Drop for GatewayRuntimeGuard {
    fn drop(&mut self) {
        if read_pid(&self.paths.pid_file) == Some(self.pid) {
            let _ = fs::remove_file(&self.paths.pid_file);
        }
        let _ = fs::remove_file(&self.paths.lock_file);
        let _ = fs::remove_file(&self.paths.stop_file);
    }
}

fn run_foreground_with_paths(paths: GatewayRuntimePaths) -> io::Result<()> {
    let guard = GatewayRuntimeGuard::acquire(paths.clone())?;
    let url = flyflor_ws_url();
    write_status(
        &paths,
        "starting",
        Some(guard.pid),
        Some(&url),
        None,
        json!({}),
    )?;
    gateway_log(
        &paths,
        format!("gateway runtime starting pid={}", guard.pid),
    )?;
    spawn_gateway_channel_runtime();
    gateway_log(&paths, "gateway channel runtime requested")?;
    println!(
        "flyflor gateway runtime running pid={} ws={} log={}",
        guard.pid,
        url,
        paths.log_file.display()
    );

    let result = FlyflorWsBridge::new(paths.clone(), url).run(guard.pid);
    match &result {
        Ok(()) => {
            write_status(&paths, "stopped", Some(guard.pid), None, None, json!({}))?;
            gateway_log(&paths, "gateway runtime stopped")?;
        }
        Err(error) => {
            write_status(
                &paths,
                "failed",
                Some(guard.pid),
                None,
                Some(&error.to_string()),
                json!({}),
            )?;
            gateway_log(&paths, format!("gateway runtime failed {error}"))?;
        }
    }
    result
}

fn start_daemon(paths: &GatewayRuntimePaths) -> io::Result<GatewayStartReport> {
    paths.ensure()?;
    if let Some(pid) = running_pid(paths) {
        return Ok(GatewayStartReport {
            pid,
            already_running: true,
            paths: GatewayRuntimePathReport::from_paths(paths),
        });
    }
    remove_stale_runtime_files(paths)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.log_file)?;
    let stderr = log.try_clone()?;
    let child = Command::new(env::current_exe()?)
        .env(FOREGROUND_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()?;
    let pid = child.id();
    wait_for_daemon_start(paths, pid)?;
    Ok(GatewayStartReport {
        pid,
        already_running: false,
        paths: GatewayRuntimePathReport::from_paths(paths),
    })
}

fn stop_daemon(paths: &GatewayRuntimePaths) -> io::Result<GatewayStopReport> {
    paths.ensure()?;
    let Some(pid) = read_pid(&paths.pid_file) else {
        return Ok(GatewayStopReport {
            pid: None,
            state: GatewayRuntimeState::Stopped,
            paths: GatewayRuntimePathReport::from_paths(paths),
        });
    };
    if !process_alive(pid) {
        remove_stale_runtime_files(paths)?;
        return Ok(GatewayStopReport {
            pid: Some(pid),
            state: GatewayRuntimeState::Stale,
            paths: GatewayRuntimePathReport::from_paths(paths),
        });
    }
    fs::write(&paths.stop_file, format!("{}\n", now_millis()))?;
    write_status(
        &paths,
        "stopping",
        Some(pid),
        None,
        None,
        json!({ "stopFile": paths.stop_file }),
    )?;
    let start = now_millis();
    while now_millis().saturating_sub(start) < STOP_WAIT.as_millis() as u64 {
        if !process_alive(pid) {
            remove_stale_runtime_files(paths)?;
            return Ok(GatewayStopReport {
                pid: Some(pid),
                state: GatewayRuntimeState::Stopped,
                paths: GatewayRuntimePathReport::from_paths(paths),
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
    terminate_process(pid)?;
    Ok(GatewayStopReport {
        pid: Some(pid),
        state: GatewayRuntimeState::Stopping,
        paths: GatewayRuntimePathReport::from_paths(paths),
    })
}

fn status_report(paths: &GatewayRuntimePaths) -> io::Result<GatewayStatusReport> {
    paths.ensure()?;
    let pid = read_pid(&paths.pid_file);
    let running = pid.is_some_and(process_alive);
    let status = fs::read_to_string(&paths.status_file).ok();
    let state = match (pid, running) {
        (Some(_), true) => GatewayRuntimeState::Running,
        (Some(_), false) => GatewayRuntimeState::Stale,
        (None, _) => GatewayRuntimeState::Stopped,
    };
    Ok(GatewayStatusReport {
        pid,
        state,
        status_json: status,
        paths: GatewayRuntimePathReport::from_paths(paths),
    })
}

struct FlyflorWsBridge {
    paths: GatewayRuntimePaths,
    url: String,
}

impl FlyflorWsBridge {
    fn new(paths: GatewayRuntimePaths, url: String) -> Self {
        Self { paths, url }
    }

    fn run(&self, pid: u32) -> io::Result<()> {
        gateway_log(&self.paths, format!("connect {}", self.url))?;
        let (mut socket, _) = connect(self.url.as_str()).map_err(io_error)?;
        configure_socket_timeout(&mut socket)?;
        gateway_log(&self.paths, "connected")?;
        write_status(
            &self.paths,
            "running",
            Some(pid),
            Some(&self.url),
            None,
            json!({
                "connectedAt": iso8601_from_millis(now_millis()),
                "inboundCount": 0_u64
            }),
        )?;
        self.send_bootstrap(&mut socket)?;
        self.read_loop(pid, &mut socket)
    }

    fn send_bootstrap(
        &self,
        socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
    ) -> io::Result<()> {
        let now = now_millis();
        let bootstrap = GatewayClientBootstrap::new(EnvelopeFactory::new("flyflor-cli-gateway"));
        for envelope in bootstrap.build(now, env!("CARGO_PKG_VERSION")) {
            let value = envelope.into_value();
            let message_type = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            socket
                .send(Message::text(value.to_string()))
                .map_err(io_error)?;
            gateway_log(&self.paths, format!("send {message_type}"))?;
        }
        Ok(())
    }

    fn read_loop(
        &self,
        pid: u32,
        socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
    ) -> io::Result<()> {
        let mut inbound_count = 0_u64;
        loop {
            if self.paths.stop_file.exists() {
                gateway_log(&self.paths, "stop file observed")?;
                return Ok(());
            }
            match socket.read() {
                Ok(Message::Text(text)) => {
                    inbound_count = inbound_count.saturating_add(1);
                    let message_type =
                        envelope_type(text.as_ref()).unwrap_or_else(|| "unknown".to_string());
                    gateway_log(&self.paths, format!("recv {message_type}"))?;
                    write_status(
                        &self.paths,
                        "running",
                        Some(pid),
                        Some(&self.url),
                        None,
                        json!({
                            "inboundCount": inbound_count,
                            "lastMessageAt": iso8601_from_millis(now_millis()),
                            "lastMessageType": message_type
                        }),
                    )?;
                }
                Ok(Message::Close(_)) => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "flyflor /ws closed",
                    ));
                }
                Ok(_) => {}
                Err(WsError::Io(error))
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => return Err(io_error(error)),
            }
        }
    }
}

fn configure_socket_timeout(
    socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
) -> io::Result<()> {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream.set_read_timeout(Some(SOCKET_READ_TIMEOUT)),
        _ => Ok(()),
    }
}

fn flyflor_ws_url() -> String {
    env::var("FLYFLOR_WS_URL").unwrap_or_else(|_| DEFAULT_WS_URL.to_string())
}

fn wait_for_daemon_start(paths: &GatewayRuntimePaths, pid: u32) -> io::Result<()> {
    let start = now_millis();
    while now_millis().saturating_sub(start) < STATUS_WAIT.as_millis() as u64 {
        if !process_alive(pid) {
            return Err(io::Error::other(format!(
                "gateway runtime exited during start; see {}",
                paths.log_file.display()
            )));
        }
        if let Ok(status) = fs::read_to_string(&paths.status_file)
            && status.contains("\"state\":\"running\"")
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    if process_alive(pid) {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "gateway runtime failed to start; see {}",
            paths.log_file.display()
        )))
    }
}

fn remove_stale_runtime_files(paths: &GatewayRuntimePaths) -> io::Result<()> {
    if let Some(pid) = read_pid(&paths.pid_file)
        && process_alive(pid)
    {
        return Ok(());
    }
    for path in [&paths.pid_file, &paths.lock_file, &paths.stop_file] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn running_pid(paths: &GatewayRuntimePaths) -> Option<u32> {
    read_pid(&paths.pid_file).filter(|pid| process_alive(*pid))
}

fn read_pid(path: &PathBuf) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse::<u32>().ok()
}

fn write_status(
    paths: &GatewayRuntimePaths,
    state: &str,
    pid: Option<u32>,
    url: Option<&str>,
    error: Option<&str>,
    extra: Value,
) -> io::Result<()> {
    paths.ensure()?;
    let mut value = json!({
        "state": state,
        "updatedAt": iso8601_from_millis(now_millis()),
        "home": paths.home,
        "pidFile": paths.pid_file,
        "lockFile": paths.lock_file,
        "statusFile": paths.status_file,
        "logFile": paths.log_file
    });
    if let Some(pid) = pid {
        value["pid"] = json!(pid);
    }
    if let Some(url) = url {
        value["wsUrl"] = json!(url);
    }
    if let Some(error) = error {
        value["error"] = json!(error);
    }
    if let Some(extra) = extra.as_object() {
        for (key, value_extra) in extra {
            value[key] = value_extra.clone();
        }
    }
    fs::write(&paths.status_file, format!("{value}\n"))
}

fn gateway_log(paths: &GatewayRuntimePaths, message: impl AsRef<str>) -> io::Result<()> {
    paths.ensure()?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.log_file)?;
    writeln!(
        file,
        "{} gateway {}",
        iso8601_from_millis(now_millis()),
        message.as_ref()
    )
}

fn read_log_tail(paths: &GatewayRuntimePaths, max_bytes: usize) -> io::Result<String> {
    paths.ensure()?;
    let bytes = fs::read(&paths.log_file).or_else(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            Ok(Vec::new())
        } else {
            Err(error)
        }
    })?;
    if bytes.len() <= max_bytes {
        return Ok(String::from_utf8_lossy(&bytes).to_string());
    }
    let start = bytes.len().saturating_sub(max_bytes);
    Ok(String::from_utf8_lossy(&bytes[start..]).to_string())
}

fn envelope_type(raw: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(raw).ok()?;
    value
        .get("type")
        .or_else(|| value.get("messageType"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn io_error(error: impl ToString) -> io::Error {
    io::Error::other(error.to_string())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn iso8601_from_millis(millis: u64) -> String {
    let seconds = (millis / 1000) as i64;
    let nanos = ((millis % 1000) * 1_000_000) as u32;
    let Ok(time) = OffsetDateTime::from_unix_timestamp(seconds) else {
        return "1970-01-01T00:00:00Z".to_string();
    };
    let Ok(time) = time.replace_nanosecond(nanos) else {
        return "1970-01-01T00:00:00Z".to_string();
    };
    time.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    unsafe { kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
fn terminate_process(pid: u32) -> io::Result<()> {
    let result = unsafe { kill(pid as i32, 15) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn terminate_process(_pid: u32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "gateway stop is only implemented on unix platforms",
    ))
}

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_envelope_type() {
        assert_eq!(
            envelope_type(r#"{"protocol":"flyflor.ws.v1","type":"event.publish"}"#),
            Some("event.publish".to_string())
        );
        assert_eq!(
            envelope_type(r#"{"messageType":"gateway.status.snapshot"}"#),
            Some("gateway.status.snapshot".to_string())
        );
    }
}
