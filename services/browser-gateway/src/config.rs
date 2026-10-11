use std::{collections::BTreeMap, net::SocketAddr};
use wonderland_game_services::{ErrorCode, ServiceError, ServiceResult};

#[derive(Clone, Debug, Default)]
pub struct DestinationAllowlist {
    pub(crate) targets: BTreeMap<String, Vec<SocketAddr>>,
}
impl DestinationAllowlist {
    /// Pin DNS answers at operator configuration time, never from a browser command.
    pub fn from_pinned(targets: BTreeMap<String, Vec<SocketAddr>>) -> ServiceResult<Self> {
        let mut validated = BTreeMap::new();
        for (name, addresses) in targets {
            let key = endpoint_key(&name)?;
            if addresses.is_empty()
                || addresses
                    .iter()
                    .any(|a| a.port() == 0 || a.ip().is_unspecified() || a.ip().is_multicast())
            {
                return Err(destination_error());
            }
            validated.insert(key, addresses);
        }
        Ok(Self { targets: validated })
    }
    pub fn selected(&self, source_address: &str) -> ServiceResult<Vec<SocketAddr>> {
        if source_address.len() > 256 {
            return Err(destination_error());
        }
        let key = endpoint_key(&format!("{source_address}101"))?;
        self.targets
            .get(&key)
            .cloned()
            .ok_or_else(destination_error)
    }
    pub async fn resolve_operator_entries(entries: &[String]) -> ServiceResult<Self> {
        let mut targets = BTreeMap::new();
        for entry in entries {
            let key = endpoint_key(entry)?;
            let addresses = tokio::net::lookup_host(key.clone())
                .await
                .map_err(|_| destination_error())?
                .collect();
            targets.insert(key, addresses);
        }
        Self::from_pinned(targets)
    }
}

fn endpoint_key(text: &str) -> ServiceResult<String> {
    if text.is_empty()
        || text.len() > 260
        || text
            .chars()
            .any(|c| c.is_whitespace() || "/\\@?#".contains(c))
    {
        return Err(destination_error());
    }
    let url = reqwest::Url::parse(&format!("tcp://{text}")).map_err(|_| destination_error())?;
    let host = url.host_str().ok_or_else(destination_error)?;
    let port = url
        .port()
        .filter(|p| *p != 0)
        .ok_or_else(destination_error)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(destination_error());
    }
    Ok(format!("{}:{port}", host.to_ascii_lowercase()))
}
fn destination_error() -> ServiceError {
    ServiceError::new(
        ErrorCode::DestinationRejected,
        "The server-selected destination is not in the operator's pinned TCP allowlist",
    )
}

#[derive(Clone, Debug)]
pub struct GatewayConfig {
    pub bind: SocketAddr,
    pub api_base_url: Option<String>,
    pub allowed_origins: Vec<String>,
    pub destinations: DestinationAllowlist,
    pub max_sessions: usize,
}
impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8787".parse().unwrap(),
            api_base_url: None,
            allowed_origins: vec![],
            destinations: DestinationAllowlist::default(),
            max_sessions: 128,
        }
    }
}
impl GatewayConfig {
    pub async fn from_env() -> ServiceResult<Self> {
        let mut config = Self::default();
        if let Ok(value) = std::env::var("WONDERLAND_GATEWAY_BIND") {
            config.bind = value.parse().map_err(|_| {
                ServiceError::new(ErrorCode::InvalidRequest, "Invalid gateway listen address")
            })?;
        }
        config.api_base_url = std::env::var("WONDERLAND_API_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty());
        config.allowed_origins = std::env::var("WONDERLAND_BROWSER_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        for origin in &config.allowed_origins {
            let url = reqwest::Url::parse(origin).map_err(|_| {
                ServiceError::new(ErrorCode::InvalidRequest, "Invalid browser origin")
            })?;
            if !matches!(url.scheme(), "http" | "https")
                || url.origin().ascii_serialization() != *origin
            {
                return Err(ServiceError::new(
                    ErrorCode::InvalidRequest,
                    "Browser origins must be exact HTTP(S) origins",
                ));
            }
        }
        let entries = std::env::var("WONDERLAND_TCP_ALLOWLIST")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        config.destinations = DestinationAllowlist::resolve_operator_entries(&entries).await?;
        if let Some(base) = &config.api_base_url {
            let url = wonderland_game_services::ApiRoutes::new(base)?;
            let local = matches!(
                url.base().host_str(),
                Some("localhost" | "127.0.0.1" | "[::1]")
            );
            if url.base().scheme() != "https" && !local {
                return Err(ServiceError::new(
                    ErrorCode::InvalidRequest,
                    "Remote account services require HTTPS",
                ));
            }
        }
        Ok(config)
    }
}
