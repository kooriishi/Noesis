use serde_json::Value;

pub trait EventSink: Send + Sync {
    fn send_event(&self, event: Value) -> Result<(), String>;
}

pub struct NullEventSink;

impl EventSink for NullEventSink {
    fn send_event(&self, _event: Value) -> Result<(), String> {
        Ok(())
    }
}
