use serde_json::Value;

use super::{
    command::GatewayCommandBuilder,
    envelope::{EnvelopeFactory, GatewayEnvelope},
    subscription::BOOTSTRAP_COMMAND_TYPES,
};

#[derive(Clone, Debug)]
pub struct GatewayClientBootstrap {
    commands: GatewayCommandBuilder,
    history_limit: u64,
    history_before_ts: Option<u64>,
    fork_memory_limit: u64,
}

impl GatewayClientBootstrap {
    pub fn new(factory: EnvelopeFactory) -> Self {
        Self {
            commands: GatewayCommandBuilder::new(factory),
            history_limit: 20,
            history_before_ts: None,
            fork_memory_limit: 5,
        }
    }

    pub fn with_limits(mut self, history_limit: u64, fork_memory_limit: u64) -> Self {
        self.history_limit = history_limit;
        self.fork_memory_limit = fork_memory_limit;
        self
    }

    pub fn with_history_before_ts(mut self, before_ts: Option<u64>) -> Self {
        self.history_before_ts = before_ts;
        self
    }

    pub fn build(&self, sequence: u64, version: &str) -> Vec<GatewayEnvelope> {
        vec![
            self.commands.client_hello(sequence, version),
            self.commands.history_list_with_before(
                sequence,
                self.history_limit,
                None,
                self.history_before_ts,
            ),
            self.commands.task_list(sequence),
            self.commands.gateway_status_get(sequence),
            self.commands
                .fork_memory_get(sequence, self.fork_memory_limit),
            self.commands.event_subscribe(sequence),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_preserves_command_order() {
        let bootstrap = GatewayClientBootstrap::new(EnvelopeFactory::new("flyflor-cli"))
            .with_limits(30, 8)
            .with_history_before_ts(Some(123));
        let envelopes = bootstrap.build(42, "0.1.0");
        let types = envelopes
            .iter()
            .filter_map(|envelope| envelope.value().get("type").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert_eq!(types, BOOTSTRAP_COMMAND_TYPES);
        assert_eq!(
            envelopes[1]
                .value()
                .get("payload")
                .and_then(|payload| payload.get("limit"))
                .and_then(Value::as_u64),
            Some(30)
        );
        assert_eq!(
            envelopes[1]
                .value()
                .get("payload")
                .and_then(|payload| payload.get("beforeTs"))
                .and_then(Value::as_u64),
            Some(123)
        );
        assert_eq!(
            envelopes[4]
                .value()
                .get("payload")
                .and_then(|payload| payload.get("limit"))
                .and_then(Value::as_u64),
            Some(8)
        );
    }
}
