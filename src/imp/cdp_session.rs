use crate::imp::{core::*, prelude::*};

#[derive(Debug)]
pub(crate) struct CdpSession {
    channel: ChannelOwner,
    tx: Mutex<Option<broadcast::Sender<Evt>>>,
}

#[derive(Debug, Clone)]
pub(crate) enum Evt {
    Event { method: String, params: Value },
    Detached,
}

impl CdpSession {
    pub(crate) fn try_new(c: ChannelOwner) -> Result<Self, Error> {
        Ok(Self { channel: c, tx: Mutex::new(None) })
    }
    
    pub(crate) async fn send(&self, method: &str, params: Option<Map<String, Value>>) -> ArcResult<Value> {
        let mut p = Map::new();
        p.insert("method".into(), Value::String(method.into()));
        if let Some(pp) = params {
            p.insert("params".into(), Value::Object(pp));
        }
        let v = send_message!(self, "send", p);
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    }
    
    pub(crate) async fn detach(&self) -> ArcResult<()> {
        let _ = send_message!(self, "detach", Map::default());
        Ok(())
    }
}

impl RemoteObject for CdpSession {
    fn channel(&self) -> &ChannelOwner { &self.channel }
    fn channel_mut(&mut self) -> &mut ChannelOwner { &mut self.channel }

    fn handle_event(&self, _ctx: &Context, method: Str<Method>, params: Map<String, Value>) -> Result<(), Error> {
        match method.as_str() {
            "event" => {
                let m = params.get("method").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                let p = params.get("params").cloned().unwrap_or(Value::Null);
                self.emit_event(Evt::Event { method: m, params: p });
            }
            "detached" => self.emit_event(Evt::Detached),
            _ => {}
        }
        Ok(())
    }
}

impl EventEmitter for CdpSession {
    type Event = Evt;
    fn tx(&self) -> Option<broadcast::Sender<Self::Event>> { self.tx.lock().unwrap().clone() }
    fn set_tx(&self, tx: broadcast::Sender<Self::Event>) { *self.tx.lock().unwrap() = Some(tx); }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventType {
    Event,
    Detached,
}

impl IsEvent for Evt {
    type EventType = EventType;
    fn event_type(&self) -> Self::EventType {
        match self {
            Self::Event { .. } => EventType::Event,
            Self::Detached => EventType::Detached,
        }
    }
}
