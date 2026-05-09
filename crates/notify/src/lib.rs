#[cfg(feature = "ssr")]
pub(crate) fn http_client() -> &'static reqwest::Client {
    static C: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    C.get_or_init(build_http_client)
}

#[cfg(feature = "ssr")]
fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to build timed notifier HTTP client");
            reqwest::Client::new()
        })
}

#[cfg(feature = "ssr")]
mod bark;
#[cfg(feature = "ssr")]
mod bus;
#[cfg(feature = "ssr")]
mod channels;
#[cfg(feature = "ssr")]
mod discord;
#[cfg(feature = "ssr")]
mod inapp;
#[cfg(feature = "ssr")]
mod smtp;
#[cfg(feature = "ssr")]
mod telegram;

#[cfg(feature = "ssr")]
pub use bus::{build_notifier, Notifier, NotifyBus};
#[cfg(feature = "ssr")]
pub use channels::{
    create_channel, delete_channel, list_channels, test_channel, NotifyChannelSummary,
    MAX_CHANNEL_NAME_CHARS,
};

#[cfg(all(test, feature = "ssr"))]
mod tests {
    #[test]
    fn notifier_http_client_builder_succeeds() {
        let _ = super::build_http_client();
    }
}
