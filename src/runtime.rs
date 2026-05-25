use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use crate::cli::{
    BinaryResolution, CliConfig, KernelMode, KernelProcessConfig, KernelProcessState, ManagedKernel,
    ProcessStatus, gateway_health_url, managed_ws_url, path_label, resolve_binary, spawn_kernel, wait_for_health,
};
use crate::gateway_core::{
    ConnectionPhase, ConnectionState, GatewayCore, GatewayCoreCommand, GatewayCoreEvent, SessionTurn, TurnStatus,
    spawn_gateway_core,
};
use crate::protocol::{AskMetadata, CapabilityCatalogPayload, GatewayStatusPayload, LoopMetadata, PlanningMetadata};
use crate::{RightPanelData, StatItem, TodoItem, Turn};

pub struct RuntimeBridge {
    pub config: CliConfig,
    pub connection_state: Option<ConnectionState>,
    pub process_state: KernelProcessState,
    pub binary_resolution: Option<BinaryResolution>,
    pub turns: Vec<Turn>,
    pub todos: Vec<TodoItem>,
    pub right_panel: RightPanelData,
    pub active_turn: Option<SessionTurn>,
    pub last_ask_snapshot: Option<AskMetadata>,
    pub last_planning_snapshot: Option<PlanningMetadata>,
    pub last_loop_snapshot: Option<LoopMetadata>,
    pub gateway: Option<GatewayCore>,
    pub managed_kernel: Option<ManagedKernel>,
}

impl RuntimeBridge {
    pub fn new(config: CliConfig) -> Self {
        Self {
            config,
            connection_state: None,
            process_state: KernelProcessState {
                status: ProcessStatus::NotStarted,
                pid: None,
                started_at: None,
                last_exit_code: None,
                stderr_tail: Vec::new(),
            },
            binary_resolution: None,
            turns: Vec::new(),
            todos: Vec::new(),
            right_panel: RightPanelData::default_live(),
            active_turn: None,
            last_ask_snapshot: None,
            last_planning_snapshot: None,
            last_loop_snapshot: None,
            gateway: None,
            managed_kernel: None,
        }
    }

    pub fn initialize(&mut self) -> Result<(), String> {
        match self.config.kernel_mode {
            KernelMode::Mock => Ok(()),
            KernelMode::RemoteWs => {
                let ws_url = self
                    .config
                    .ws_url
                    .clone()
                    .ok_or_else(|| String::from("missing --ws-url for remote mode"))?;
                self.gateway = Some(spawn_gateway_core(ws_url));
                self.connect_gateway();
                Ok(())
            }
            KernelMode::ManagedLocalBinary => {
                if self.config.host != "127.0.0.1" || self.config.port != 8787 {
                    return Err(String::from(
                        "managed-local currently requires host 127.0.0.1 and port 8787; flyflor binary host/port overrides are not wired yet",
                    ));
                }
                let resolution = resolve_binary(&self.config).map_err(|error| error.to_string())?;
                let kernel_config = KernelProcessConfig {
                    binary_path: resolution.resolved_path.clone(),
                    args: vec!["gateway".to_string()],
                    host: self.config.host.clone(),
                    port: self.config.port,
                };
                let mut kernel = spawn_kernel(&kernel_config).map_err(|error| error.to_string())?;
                wait_for_health(&gateway_health_url(&self.config), Duration::from_secs(15))
                    .map_err(|error| error.to_string())?;
                kernel.mark_running();
                self.process_state = kernel.state.clone();
                self.binary_resolution = Some(resolution);
                self.gateway = Some(spawn_gateway_core(managed_ws_url(&self.config)));
                self.managed_kernel = Some(kernel);
                self.connect_gateway();
                Ok(())
            }
        }
    }

    pub fn tick(&mut self) {
        if let Some(kernel) = &mut self.managed_kernel {
            kernel.poll_exit();
            self.process_state = kernel.state.clone();
        }

        loop {
            let Some(gateway) = &self.gateway else {
                break;
            };
            match gateway.events.try_recv() {
                Ok(event) => self.apply_gateway_event(event),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        self.refresh_right_panel();
    }

    pub fn send_input(&mut self, text: String) -> Result<(), String> {
        if matches!(self.config.kernel_mode, KernelMode::Mock) {
            return Ok(());
        }
        if self.active_turn.is_some() {
            return Err(String::from("another request is still active"));
        }
        let turn = SessionTurn {
            request_id: format!("local-{}", self.turns.len() + 1),
            message_id: format!("local-{}", self.turns.len() + 1),
            user_text: text.clone(),
            assistant_text: String::new(),
            status: TurnStatus::Pending,
            ask_snapshot: None,
            planning_snapshot: None,
            loop_snapshot: None,
            error: None,
        };
        self.turns.push(Turn {
            user: text,
            thought: None,
            answer: String::new(),
            footer: String::from("connecting..."),
        });
        self.active_turn = Some(turn);
        if let Some(gateway) = &self.gateway {
            gateway
                .commands
                .send(GatewayCoreCommand::SendMessage {
                    text: self.turns.last().map(|turn| turn.user.clone()).unwrap_or_default(),
                    user_id: self.config.user_id.clone(),
                    display_name: self.config.display_name.clone(),
                })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn add_mock_turn(&mut self, text: String) {
        let turn_index = self.turns.len() + 1;
        self.turns.push(crate::build_runtime_turn(turn_index, text));
    }

    pub fn shutdown(&mut self) {
        if let Some(gateway) = &self.gateway {
            let _ = gateway.commands.send(GatewayCoreCommand::Disconnect);
        }
        if let Some(kernel) = &mut self.managed_kernel {
            kernel.kill();
            self.process_state = kernel.state.clone();
        }
    }

    pub fn header_status(&self) -> String {
        let phase = self
            .connection_state
            .as_ref()
            .map(|state| format!("{:?}", state.phase).to_lowercase())
            .unwrap_or_else(|| String::from("ready"));
        match self.config.kernel_mode {
            KernelMode::Mock => String::from("flyflor · mock"),
            KernelMode::ManagedLocalBinary => format!("flyflor · {phase}"),
            KernelMode::RemoteWs => format!("remote ws · {phase}"),
        }
    }

    fn connect_gateway(&mut self) {
        if let Some(gateway) = &self.gateway {
            let _ = gateway.commands.send(GatewayCoreCommand::Connect);
        }
    }

    fn apply_gateway_event(&mut self, event: GatewayCoreEvent) {
        match event {
            GatewayCoreEvent::ConnectionChanged(state) => {
                self.connection_state = Some(state);
            }
            GatewayCoreEvent::ServerHello(payload) => {
                self.connection_state = Some(ConnectionState {
                    phase: ConnectionPhase::Ready,
                    client_id: Some(payload.client_id.clone()),
                    connected_at: Some(payload.connected_at.clone()),
                    last_pong_at: None,
                    last_error: None,
                    ws_url: self
                        .config
                        .ws_url
                        .clone()
                        .unwrap_or_else(|| managed_ws_url(&self.config)),
                    hello_snapshot: Some(payload.clone()),
                    gateway_status_snapshot: Some(payload.status.clone()),
                    capability_catalog_snapshot: None,
                    subscription_snapshot: Vec::new(),
                });
            }
            GatewayCoreEvent::Ack(payload) => {
                if let Some(state) = &mut self.connection_state {
                    state.subscription_snapshot = payload.subscriptions;
                }
            }
            GatewayCoreEvent::GatewayStatusSnapshot(payload) => {
                self.update_gateway_status(payload);
            }
            GatewayCoreEvent::CapabilityCatalogSnapshot(payload) => {
                self.update_catalog(payload);
            }
            GatewayCoreEvent::TurnDelta { payload, .. } => {
                if let Some(active) = &mut self.active_turn {
                    active.status = TurnStatus::Streaming;
                    active.assistant_text.push_str(&payload.delta);
                    if let Some(turn) = self.turns.last_mut() {
                        turn.answer.push_str(&payload.delta);
                        turn.footer = String::from("streaming...");
                    }
                }
            }
            GatewayCoreEvent::TurnFinal { payload, .. } => {
                let ask_snapshot = payload.reply.metadata.as_ref().and_then(|meta| meta.ask.clone());
                let planning_snapshot = payload.reply.metadata.as_ref().and_then(|meta| meta.planning.clone());
                let loop_snapshot = extract_loop(payload.reply.metadata.as_ref());
                self.last_ask_snapshot = ask_snapshot.clone();
                self.last_planning_snapshot = planning_snapshot.clone();
                self.last_loop_snapshot = loop_snapshot.clone();
                self.todos = planning_to_todos(planning_snapshot.as_ref());
                if let Some(active) = &mut self.active_turn {
                    active.status = TurnStatus::Final;
                    active.assistant_text = payload.reply.text.clone();
                    active.ask_snapshot = ask_snapshot;
                    active.planning_snapshot = planning_snapshot;
                    active.loop_snapshot = loop_snapshot;
                    if let Some(turn) = self.turns.last_mut() {
                        turn.answer = payload.reply.text;
                        turn.footer = build_turn_footer(active);
                    }
                    self.active_turn = None;
                }
            }
            GatewayCoreEvent::TurnError { payload, .. } => {
                if let Some(active) = &mut self.active_turn {
                    active.status = TurnStatus::Failed;
                    active.error = Some(payload.message.clone());
                    if let Some(turn) = self.turns.last_mut() {
                        turn.answer = payload.message;
                        turn.footer = String::from("turn error");
                    }
                    self.active_turn = None;
                }
            }
            GatewayCoreEvent::ControlError(payload) => {
                if let Some(state) = &mut self.connection_state {
                    state.last_error = Some(format!("{}: {}", payload.code, payload.message));
                }
            }
            GatewayCoreEvent::RuntimeEvent { .. } => {}
        }
    }

    fn refresh_right_panel(&mut self) {
        let mode = match self.config.kernel_mode {
            KernelMode::Mock => "mock",
            KernelMode::ManagedLocalBinary => "managed-local",
            KernelMode::RemoteWs => "remote-ws",
        };
        let phase = self
            .connection_state
            .as_ref()
            .map(|state| format!("{:?}", state.phase).to_lowercase())
            .unwrap_or_else(|| String::from("idle"));
        let binary = self
            .binary_resolution
            .as_ref()
            .map(|resolution| path_label(&resolution.resolved_path))
            .unwrap_or_else(|| String::from("n/a"));
        let endpoint = self
            .config
            .ws_url
            .clone()
            .unwrap_or_else(|| managed_ws_url(&self.config));
        let client_id = self
            .connection_state
            .as_ref()
            .and_then(|state| state.client_id.clone())
            .unwrap_or_else(|| String::from("-"));
        let subscriptions = self
            .connection_state
            .as_ref()
            .map(|state| state.subscription_snapshot.len().to_string())
            .unwrap_or_else(|| String::from("0"));
        let active = if self.active_turn.is_some() { "yes" } else { "no" };
        let ask_prompt = self
            .current_ask()
            .map(|ask| ask.prompt.clone())
            .unwrap_or_else(|| String::from("No pending ask"));
        let loop_text = self
            .current_loop()
            .map(|snapshot| {
                format!(
                    "{} · step {}",
                    snapshot
                        .loop_guard_reason
                        .clone()
                        .unwrap_or_else(|| snapshot.stop.clone()),
                    snapshot.step_count
                )
            })
            .unwrap_or_else(|| String::from("No pending loop"));

        self.right_panel = RightPanelData {
            thinking_label: mode.to_string(),
            blackboard_status: format!("connection: {phase}"),
            blackboard_stream: vec![
                format!("ASK: {ask_prompt}"),
                format!("Loop: {loop_text}"),
                format!("binary: {binary}"),
                format!("endpoint: {endpoint}"),
                format!("active request: {active}"),
            ],
            model_stats: vec![
                StatItem {
                    label: String::from("mode"),
                    value: mode.to_string(),
                },
                StatItem {
                    label: String::from("clientId"),
                    value: client_id,
                },
            ],
            token_stats: vec![
                StatItem {
                    label: String::from("subs"),
                    value: subscriptions,
                },
                StatItem {
                    label: String::from("proc"),
                    value: process_status_label(&self.process_state),
                },
            ],
            context_total: String::from("planning"),
            context_percent: format!("{}", self.todos.len()),
            context_bar: build_bar(self.todos.len()),
            context_usage: format!("{} todo items", self.todos.len()),
            context_ratio: 0.0,
            context_used_tokens: self.todos.len(),
            context_max_tokens: None,
            context_used: self.todos.len().to_string(),
            context_max: String::from("未知"),
            fork_memory: Default::default(),
            footer: format!("{} · {}", mode, phase),
        };
    }

    fn update_gateway_status(&mut self, payload: GatewayStatusPayload) {
        if let Some(state) = &mut self.connection_state {
            state.gateway_status_snapshot = Some(payload);
        }
    }

    fn update_catalog(&mut self, payload: CapabilityCatalogPayload) {
        if let Some(state) = &mut self.connection_state {
            state.capability_catalog_snapshot = Some(payload);
        }
    }

    fn last_ask(&self) -> Option<&AskMetadata> {
        self.active_turn
            .as_ref()
            .and_then(|turn| turn.ask_snapshot.as_ref())
            .or(self.last_ask_snapshot.as_ref())
    }

    fn last_loop(&self) -> Option<&LoopMetadata> {
        self.active_turn
            .as_ref()
            .and_then(|turn| turn.loop_snapshot.as_ref())
            .or(self.last_loop_snapshot.as_ref())
    }

    fn current_ask(&self) -> Option<&AskMetadata> {
        self.last_ask()
    }

    fn current_loop(&self) -> Option<&LoopMetadata> {
        self.last_loop()
    }
}

fn extract_loop(metadata: Option<&crate::protocol::ReplyMetadata>) -> Option<LoopMetadata> {
    metadata
        .and_then(|meta| meta.executive_tool_loop.clone())
        .or_else(|| metadata.and_then(|meta| meta.ask.as_ref().and_then(|ask| ask.executive_tool_loop.clone())))
}

fn planning_to_todos(planning: Option<&PlanningMetadata>) -> Vec<TodoItem> {
    let Some(planning) = planning else {
        return Vec::new();
    };
    planning
        .task_plans
        .iter()
        .flat_map(|plan| {
            plan.steps.iter().map(|step| TodoItem {
                marker: if step.status == "completed" {
                    "●".to_string()
                } else {
                    "○".to_string()
                },
                label: step.title.clone(),
                status: step.status.clone(),
                active: step.status == "in_progress" || step.status == "active" || step.status == "planned",
            })
        })
        .collect()
}

fn build_turn_footer(turn: &SessionTurn) -> String {
    match turn.status {
        TurnStatus::Final => String::from("flyflor · final"),
        TurnStatus::Streaming => String::from("flyflor · streaming"),
        TurnStatus::Failed => String::from("flyflor · failed"),
        TurnStatus::Pending => String::from("flyflor · pending"),
    }
}

fn process_status_label(state: &KernelProcessState) -> String {
    match state.status {
        ProcessStatus::Starting => String::from("starting"),
        ProcessStatus::Running => String::from("running"),
        ProcessStatus::Exited => String::from("exited"),
        ProcessStatus::NotStarted => String::from("none"),
    }
}

fn build_bar(count: usize) -> String {
    let filled = count.min(10);
    format!("{}{}", "■".repeat(filled), "□".repeat(10usize.saturating_sub(filled)))
}
