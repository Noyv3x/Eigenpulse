//! Trusted reverse-proxy resolution shared by login throttling and CSRF.
//!
//! Forwarded headers are attacker-controlled unless the TCP peer is explicitly
//! trusted. `X-Forwarded-For` is walked from right to left: trusted proxy hops
//! are peeled away until the first untrusted address, which is the client.

use axum::http::HeaderMap;
use ipnet::IpNet;
use std::net::{IpAddr, SocketAddr};
use std::sync::OnceLock;

const TRUSTED_PROXY_ENV: &str = "EP_TRUSTED_PROXY_CIDRS";

#[derive(Clone, Debug, Default)]
pub struct TrustedProxies {
    networks: Vec<IpNet>,
}

impl TrustedProxies {
    pub fn parse(raw: &str) -> anyhow::Result<Self> {
        let mut networks = Vec::new();
        for value in raw.split(',').map(str::trim).filter(|v| !v.is_empty()) {
            let network = value.parse::<IpNet>().map_err(|error| {
                anyhow::anyhow!("invalid {TRUSTED_PROXY_ENV} entry {value:?}: {error}")
            })?;
            if network.prefix_len() == 0 {
                anyhow::bail!(
                    "{TRUSTED_PROXY_ENV} entry {value:?} trusts the entire address family"
                );
            }
            networks.push(network);
        }
        Ok(Self { networks })
    }

    pub fn from_env() -> anyhow::Result<Self> {
        match std::env::var(TRUSTED_PROXY_ENV) {
            Ok(raw) => Self::parse(&raw),
            Err(std::env::VarError::NotPresent) => Ok(Self::default()),
            Err(std::env::VarError::NotUnicode(_)) => {
                anyhow::bail!("{TRUSTED_PROXY_ENV} must be valid UTF-8")
            }
        }
    }

    pub fn is_trusted(&self, address: IpAddr) -> bool {
        self.networks
            .iter()
            .any(|network| network.contains(&address))
    }

    /// Resolve a client address without trusting any header supplied directly
    /// by an untrusted peer. A malformed chain is ignored in its entirety.
    pub fn client_ip(&self, headers: &HeaderMap, peer: SocketAddr) -> IpAddr {
        if !self.is_trusted(peer.ip()) {
            return peer.ip();
        }
        let mut values = headers.get_all("x-forwarded-for").iter();
        let Some(raw) = values.next().and_then(|value| value.to_str().ok()) else {
            return peer.ip();
        };
        if values.next().is_some() {
            return peer.ip();
        }
        let hops = raw
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::parse::<IpAddr>)
            .collect::<Result<Vec<_>, _>>();
        let Ok(hops) = hops else { return peer.ip() };

        let mut resolved = peer.ip();
        for hop in hops.into_iter().rev() {
            if !self.is_trusted(resolved) {
                break;
            }
            resolved = hop;
        }
        resolved
    }

    /// Return a forwarded scheme only for a trusted direct peer. Multiple or
    /// malformed values are rejected: deployment examples deliberately make
    /// the edge proxy overwrite this header instead of appending to it.
    pub fn forwarded_proto<'a>(
        &self,
        headers: &'a HeaderMap,
        peer: Option<SocketAddr>,
    ) -> Option<&'a str> {
        let peer = peer?;
        if !self.is_trusted(peer.ip()) {
            return None;
        }
        let mut values = headers.get_all("x-forwarded-proto").iter();
        let value = values.next()?.to_str().ok()?.trim();
        if values.next().is_some() {
            return None;
        }
        if value.contains(',') || !matches!(value, "http" | "https") {
            return None;
        }
        Some(value)
    }

    pub fn is_empty(&self) -> bool {
        self.networks.is_empty()
    }
}

static TRUSTED_PROXIES: OnceLock<TrustedProxies> = OnceLock::new();

/// Parse and freeze proxy trust at startup. Invalid CIDRs fail startup instead
/// of silently weakening throttling or origin checks.
pub fn init_trusted_proxies_from_env() -> anyhow::Result<&'static TrustedProxies> {
    if let Some(config) = TRUSTED_PROXIES.get() {
        return Ok(config);
    }
    let config = TrustedProxies::from_env()?;
    let _ = TRUSTED_PROXIES.set(config);
    Ok(TRUSTED_PROXIES
        .get()
        .expect("trusted proxy configuration initialized"))
}

pub fn trusted_proxies() -> &'static TrustedProxies {
    TRUSTED_PROXIES.get_or_init(|| {
        TrustedProxies::from_env()
            .unwrap_or_else(|error| panic!("failed to parse trusted proxy configuration: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::TrustedProxies;
    use axum::http::HeaderMap;
    use std::net::SocketAddr;

    fn headers(xff: &str, proto: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", xff.parse().unwrap());
        headers.insert("x-forwarded-proto", proto.parse().unwrap());
        headers
    }

    #[test]
    fn untrusted_peer_cannot_spoof_forwarded_headers() {
        let config = TrustedProxies::parse("10.0.0.0/8").unwrap();
        let peer: SocketAddr = "203.0.113.9:1234".parse().unwrap();
        let headers = headers("192.0.2.4", "https");
        assert_eq!(config.client_ip(&headers, peer), peer.ip());
        assert_eq!(config.forwarded_proto(&headers, Some(peer)), None);
    }

    #[test]
    fn walks_appended_chain_from_right_to_left() {
        let config = TrustedProxies::parse("10.0.0.0/8, 192.168.0.0/16").unwrap();
        let peer: SocketAddr = "10.0.0.2:1234".parse().unwrap();
        // 198.51.100.7 was injected by the browser. 203.0.113.8 is the real
        // untrusted client appended by the first trusted proxy.
        let headers = headers("198.51.100.7, 203.0.113.8, 192.168.1.5", "https");
        assert_eq!(config.client_ip(&headers, peer).to_string(), "203.0.113.8");
        assert_eq!(config.forwarded_proto(&headers, Some(peer)), Some("https"));
    }

    #[test]
    fn malformed_or_ambiguous_headers_fail_closed() {
        let config = TrustedProxies::parse("10.0.0.0/8").unwrap();
        let peer: SocketAddr = "10.0.0.2:1234".parse().unwrap();
        let malformed = headers("203.0.113.8, not-an-ip", "https, http");
        assert_eq!(config.client_ip(&malformed, peer), peer.ip());
        assert_eq!(config.forwarded_proto(&malformed, Some(peer)), None);
    }

    #[test]
    fn parses_ipv4_and_ipv6_cidrs() {
        let config = TrustedProxies::parse("127.0.0.1/32, ::1/128").unwrap();
        assert!(config.is_trusted("127.0.0.1".parse().unwrap()));
        assert!(config.is_trusted("::1".parse().unwrap()));
        assert!(TrustedProxies::parse("definitely-not-a-cidr").is_err());
        assert!(TrustedProxies::parse("0.0.0.0/0").is_err());
        assert!(TrustedProxies::parse("::/0").is_err());
    }
}
