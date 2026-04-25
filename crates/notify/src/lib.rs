#[cfg(feature = "ssr")]
pub(crate) fn http_client() -> &'static reqwest::Client {
    static C: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    C.get_or_init(reqwest::Client::new)
}

#[cfg(feature = "ssr")]
pub mod bus;
#[cfg(feature = "ssr")]
pub mod inapp;
#[cfg(feature = "ssr")]
pub mod smtp;
#[cfg(feature = "ssr")]
pub mod bark;
#[cfg(feature = "ssr")]
pub mod telegram;
#[cfg(feature = "ssr")]
pub mod discord;
#[cfg(feature = "ssr")]
pub mod channels;

#[cfg(feature = "ssr")]
pub use bus::{NotifyBus, build_notifier, Notifier};
#[cfg(feature = "ssr")]
pub use channels::{NotifyChannelRow, list_channels, create_channel, update_channel, delete_channel, test_channel};
