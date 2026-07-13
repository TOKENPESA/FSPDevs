use serde_json::Value;
use tokio::sync::mpsc;

/// Outbound MFA control-plane bus for sidecar system events (hardware profile, etc.).
#[derive(Clone)]
pub struct MfaControlBus {
    sys_broadcast_tx: mpsc::Sender<String>,
}

impl MfaControlBus {
    pub fn channel() -> (Self, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(32);
        (Self { sys_broadcast_tx: tx }, rx)
    }

    pub fn try_publish_sys_broadcast(&self, payload: Value) -> Result<(), String> {
        let mut envelope = serde_json::Map::new();
        envelope.insert(
            "type".to_string(),
            Value::String("sys_broadcast".to_string()),
        );
        if let Value::Object(map) = payload {
            for (key, value) in map {
                envelope.insert(key, value);
            }
        } else {
            return Err("sys_broadcast payload must be a JSON object".to_string());
        }

        let text = serde_json::to_string(&Value::Object(envelope))
            .map_err(|err| format!("sys_broadcast serialization failed: {err}"))?;

        self.sys_broadcast_tx
            .try_send(text)
            .map_err(|err| format!("MFA sys_broadcast channel unavailable: {err}"))
    }
}
