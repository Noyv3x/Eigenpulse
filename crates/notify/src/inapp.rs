use crate::Notifier;
use async_trait::async_trait;
use ep_core::NotifyMessage;

pub struct InappNotifier;

#[async_trait]
impl Notifier for InappNotifier {
    fn kind(&self) -> &'static str { "inapp" }
    async fn send(&self, _msg: &NotifyMessage) -> anyhow::Result<()> {
        // In-app delivery is handled by `NotifyBus::dispatch` (writes notification + broadcasts).
        Ok(())
    }
}
