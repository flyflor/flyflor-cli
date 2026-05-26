use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub const WS_PROTOCOL: &str = "flyflor.ws.v1";

#[derive(Clone, Debug)]
pub struct GatewayEnvelope {
    value: Value,
}

impl GatewayEnvelope {
    pub fn into_value(self) -> Value {
        self.value
    }
}

#[derive(Clone, Debug)]
pub struct EnvelopeFactory {
    source: String,
}

impl EnvelopeFactory {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }

    pub fn build(&self, message_type: &str, sequence: u64, payload: Value) -> GatewayEnvelope {
        self.build_with_request(message_type, sequence, None, payload)
    }

    pub fn build_with_request(
        &self,
        message_type: &str,
        sequence: u64,
        request_id: Option<String>,
        payload: Value,
    ) -> GatewayEnvelope {
        let request_id = request_id.unwrap_or_else(|| {
            format!(
                "{}-{}-{sequence}",
                self.source,
                message_type_slug(message_type)
            )
        });
        let envelope_id = format!("env-{request_id}");
        GatewayEnvelope {
            value: serde_json::json!({
                "protocol": WS_PROTOCOL,
                "id": envelope_id,
                "type": message_type,
                "at": iso8601_from_millis(sequence),
                "requestId": request_id,
                "payload": payload
            }),
        }
    }
}

fn message_type_slug(message_type: &str) -> String {
    message_type.replace('.', "-")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_standard_ws_envelope() {
        let factory = EnvelopeFactory::new("flyflor-cli");
        let envelope = factory.build(
            "gateway.status.get",
            1_770_000_000_123,
            serde_json::json!({}),
        );
        let value = envelope.into_value();

        assert_eq!(
            value.get("protocol").and_then(Value::as_str),
            Some(WS_PROTOCOL)
        );
        assert_eq!(
            value.get("id").and_then(Value::as_str),
            Some("env-flyflor-cli-gateway-status-get-1770000000123")
        );
        assert_eq!(
            value.get("requestId").and_then(Value::as_str),
            Some("flyflor-cli-gateway-status-get-1770000000123")
        );
        assert_eq!(
            value.get("type").and_then(Value::as_str),
            Some("gateway.status.get")
        );
        assert_eq!(
            value.get("at").and_then(Value::as_str),
            Some("2026-02-02T02:40:00.123Z")
        );
    }
}
