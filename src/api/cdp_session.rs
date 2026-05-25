pub use crate::imp::cdp_session::EventType;
use crate::{
    imp::{
        cdp_session::{CdpSession as Impl, Evt},
        core::*,
        prelude::*,
    },
    Error,
};

#[derive(Debug, Clone)]
pub struct CdpSession {
    inner: Weak<Impl>,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// (cdp_method, cdp_params)
    Event(String, Value),
    Detached,
}

impl From<Evt> for Event {
    fn from(e: Evt) -> Self {
        match e {
            Evt::Event { method, params } => Event::Event(method, params),
            Evt::Detached => Event::Detached,
        }
    }
}

impl CdpSession {
    pub(crate) fn new(inner: Weak<Impl>) -> Self { Self { inner } }
    
    pub async fn send(&self, method: &str, params: Option<Map<String, Value>>) -> ArcResult<Value> {
        upgrade(&self.inner)?.send(method, params).await
    }
    
    pub async fn detach(&self) -> ArcResult<()> {
        upgrade(&self.inner)?.detach().await
    }
    
    subscribe_event! {}
}
