//! Destination validation policy for outbound HTTP requests.

use std::net::{Ipv4Addr, Ipv6Addr};

use reqwest::Url;
use rskit_errors::{AppError, AppResult};

const DEFAULT_SCHEMES: [&str; 2] = ["http", "https"];
const METADATA_HOSTS: [&str; 4] = [
    "169.254.169.254",
    "metadata.google.internal",
    "metadata",
    "100.100.100.200",
];

/// Destination policy applied to initial request URLs and redirect targets.
///
/// By default, HTTP and HTTPS URLs are allowed while link-local and common
/// cloud metadata destinations are blocked. `allowed_hosts` is an allow-list:
/// when it is empty, any non-blocked host is allowed; when it is non-empty, the
/// URL host must match one of the configured entries. Host entries are exact
/// matches unless they start with `*.`, in which case only dot-boundary
/// subdomains match (for example, `*.example.com` matches `api.example.com` but
/// not `badexample.com` or the apex `example.com`).
///
/// Hostname validation happens before DNS resolution. Use `allowed_hosts` for
/// high-trust clients that must not follow attacker-controlled DNS names.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DestinationPolicy {
    /// Allowed URL schemes. Empty means all schemes are rejected.
    pub allowed_schemes: Vec<String>,
    /// Optional host allow-list. Empty means any non-blocked host is allowed.
    pub allowed_hosts: Vec<String>,
    /// Block IPv4/IPv6 link-local address literals.
    pub block_link_local: bool,
    /// Block common cloud metadata hosts and address literals.
    pub block_metadata: bool,
}

impl DestinationPolicy {
    /// Create the default destination policy.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace allowed schemes.
    #[must_use]
    pub fn with_allowed_schemes<I, S>(mut self, schemes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_schemes = schemes.into_iter().map(Into::into).collect();
        self
    }

    /// Replace the host allow-list.
    #[must_use]
    pub fn with_allowed_hosts<I, S>(mut self, hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_hosts = hosts.into_iter().map(Into::into).collect();
        self
    }

    /// Set whether link-local address literals are blocked.
    #[must_use]
    pub fn with_block_link_local(mut self, block: bool) -> Self {
        self.block_link_local = block;
        self
    }

    /// Set whether common cloud metadata hosts are blocked.
    #[must_use]
    pub fn with_block_metadata(mut self, block: bool) -> Self {
        self.block_metadata = block;
        self
    }

    /// Validate a request or redirect URL against this policy.
    ///
    /// # Errors
    /// Returns an error when the URL scheme, host, or address literal violates
    /// the configured policy.
    pub fn validate(&self, url: &Url) -> AppResult<()> {
        self.validate_scheme(url.scheme())?;
        let host = url
            .host_str()
            .ok_or_else(|| AppError::invalid_input("url", "URL must include a host"))?;
        self.validate_host(host)
    }

    fn validate_scheme(&self, scheme: &str) -> AppResult<()> {
        if self
            .allowed_schemes
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(scheme))
        {
            Ok(())
        } else {
            Err(AppError::invalid_input(
                "url.scheme",
                format!("URL scheme '{scheme}' is not allowed"),
            ))
        }
    }

    fn validate_host(&self, host: &str) -> AppResult<()> {
        let normalized = normalize_host(host);
        if self.block_metadata && is_metadata_host(&normalized) {
            return Err(AppError::invalid_input(
                "url.host",
                "metadata service destinations are blocked",
            ));
        }
        if self.block_link_local && is_link_local_literal(&normalized) {
            return Err(AppError::invalid_input(
                "url.host",
                "link-local destinations are blocked",
            ));
        }
        if !self.allowed_hosts.is_empty()
            && !self
                .allowed_hosts
                .iter()
                .any(|allowed| host_matches(&normalized, &normalize_host(allowed)))
        {
            return Err(AppError::invalid_input(
                "url.host",
                format!("URL host '{host}' is not allowed"),
            ));
        }
        Ok(())
    }
}

impl Default for DestinationPolicy {
    fn default() -> Self {
        Self {
            allowed_schemes: DEFAULT_SCHEMES.into_iter().map(str::to_string).collect(),
            allowed_hosts: Vec::new(),
            block_link_local: true,
            block_metadata: true,
        }
    }
}

fn normalize_host(host: &str) -> String {
    host.trim_matches(['[', ']'])
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn host_matches(host: &str, allowed: &str) -> bool {
    if let Some(suffix) = allowed.strip_prefix("*.") {
        host.len() > suffix.len()
            && host.ends_with(suffix)
            && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
    } else {
        host == allowed
    }
}

fn is_metadata_host(host: &str) -> bool {
    METADATA_HOSTS
        .into_iter()
        .any(|metadata| host.eq_ignore_ascii_case(metadata))
        || host.parse::<Ipv4Addr>().is_ok_and(is_metadata_ipv4)
        || host.parse::<Ipv6Addr>().is_ok_and(|ip| {
            ip == Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254)
                || ipv6_embedded_ipv4(ip).is_some_and(is_metadata_ipv4)
        })
}

fn is_link_local_literal(host: &str) -> bool {
    host.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_link_local())
        || host.parse::<Ipv6Addr>().is_ok_and(|ip| {
            ip.is_unicast_link_local()
                || ipv6_embedded_ipv4(ip).is_some_and(|ipv4| ipv4.is_link_local())
        })
}

fn is_metadata_ipv4(ip: Ipv4Addr) -> bool {
    METADATA_HOSTS
        .into_iter()
        .filter_map(|metadata| metadata.parse::<Ipv4Addr>().ok())
        .any(|metadata| ip == metadata)
}

fn ipv6_embedded_ipv4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let octets = ip.octets();
    let is_mapped =
        octets[..10].iter().all(|octet| *octet == 0) && octets[10] == 0xff && octets[11] == 0xff;
    let is_compatible = octets[..12].iter().all(|octet| *octet == 0);

    if is_mapped || is_compatible {
        Some(Ipv4Addr::new(
            octets[12], octets[13], octets[14], octets[15],
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_blocks_metadata_ip_literal() {
        let url = Url::parse("http://169.254.169.254/latest/meta-data").unwrap();

        assert!(DestinationPolicy::new().validate(&url).is_err());
    }

    #[test]
    fn default_policy_blocks_ipv4_mapped_metadata_literal() {
        let url = Url::parse("http://[::ffff:169.254.169.254]/latest/meta-data").unwrap();

        assert!(DestinationPolicy::new().validate(&url).is_err());
    }

    #[test]
    fn default_policy_blocks_ipv4_compatible_metadata_literal() {
        let url = Url::parse("http://[::169.254.169.254]/latest/meta-data").unwrap();

        assert!(DestinationPolicy::new().validate(&url).is_err());
    }

    #[test]
    fn default_policy_blocks_link_local_literal() {
        let url = Url::parse("http://[fe80::1]/").unwrap();

        assert!(DestinationPolicy::new().validate(&url).is_err());
    }

    #[test]
    fn default_policy_blocks_ipv4_mapped_link_local_literal() {
        let url = Url::parse("http://[::ffff:169.254.1.2]/").unwrap();

        assert!(DestinationPolicy::new().validate(&url).is_err());
    }

    #[test]
    fn default_policy_blocks_ipv4_compatible_link_local_literal() {
        let url = Url::parse("http://[::169.254.1.2]/").unwrap();

        assert!(DestinationPolicy::new().validate(&url).is_err());
    }

    #[test]
    fn allow_list_rejects_non_matching_host() {
        let policy = DestinationPolicy::new().with_allowed_hosts(["api.example.com"]);
        let url = Url::parse("https://other.example.com/").unwrap();

        assert!(policy.validate(&url).is_err());
    }

    #[test]
    fn wildcard_allow_list_uses_dot_boundary() {
        let policy = DestinationPolicy::new().with_allowed_hosts(["*.example.com"]);

        assert!(
            policy
                .validate(&Url::parse("https://api.example.com/").unwrap())
                .is_ok()
        );
        assert!(
            policy
                .validate(&Url::parse("https://badexample.com/").unwrap())
                .is_err()
        );
        assert!(
            policy
                .validate(&Url::parse("https://example.com/").unwrap())
                .is_err()
        );
    }
}
