//! Explicit operator configuration. A failed connection never becomes a preview.
use serde::{Deserialize, Serialize};

pub const MAX_CONFIG_BYTES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientMode {
    Preview,
    Connected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfiguration {
    pub version: u32,
    pub mode: ClientMode,
    pub gateway_url: Option<String>,
    #[serde(default)]
    pub native_lots: bool,
}

pub fn parse_configuration(input: &str) -> Result<RuntimeConfiguration, String> {
    if input.len() > MAX_CONFIG_BYTES {
        return Err("The game configuration is too large.".into());
    }
    let config: RuntimeConfiguration = serde_json::from_str(input)
        .map_err(|_| "The game configuration is not valid.".to_string())?;
    if config.version != 1 {
        return Err("This game configuration needs a compatible client.".into());
    }
    match (config.mode, config.gateway_url.as_deref()) {
        (ClientMode::Preview, None) => {}
        (ClientMode::Connected, Some(gateway)) if valid_gateway(gateway) => {}
        _ => {
            return Err(
                "The game configuration needs an explicit mode and a valid connection address."
                    .into(),
            );
        }
    }
    if config.native_lots && config.mode != ClientMode::Connected {
        return Err("Native lots require connected mode.".into());
    }
    Ok(config)
}

fn valid_gateway(gateway: &str) -> bool {
    if gateway.is_empty()
        || gateway.trim() != gateway
        || gateway.chars().any(char::is_control)
        || gateway.contains(['\\', '?', '#'])
    {
        return false;
    }
    if gateway.starts_with('/') {
        return !gateway.starts_with("//");
    }
    let Ok(url) = url::Url::parse(gateway) else {
        return false;
    };
    if url.host().is_none() || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    match url.scheme() {
        "https" => true,
        "http" => match url.host() {
            Some(url::Host::Domain("localhost")) => true,
            Some(url::Host::Ipv4(address)) => address == std::net::Ipv4Addr::LOCALHOST,
            Some(url::Host::Ipv6(address)) => address == std::net::Ipv6Addr::LOCALHOST,
            _ => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_preview_does_not_inherit_a_live_endpoint() {
        let config =
            parse_configuration(r#"{"version":1,"mode":"preview","gateway_url":null}"#).unwrap();
        assert_eq!(config.mode, ClientMode::Preview);
        assert_eq!(config.gateway_url, None);
    }

    #[test]
    fn configured_gateway_is_not_replaced_by_a_default_server() {
        let config = parse_configuration(
            r#"{"version":1,"mode":"connected","gateway_url":"https://gateway.example/game"}"#,
        )
        .unwrap();
        assert_eq!(config.mode, ClientMode::Connected);
        assert_eq!(
            config.gateway_url.as_deref(),
            Some("https://gateway.example/game")
        );
    }

    #[test]
    fn same_origin_and_local_development_gateways_are_valid() {
        for gateway in [
            "/gateway",
            "http://localhost:4174",
            "http://127.0.0.1:4174",
            "http://[::1]:4174",
        ] {
            let input = serde_json::json!({"version":1,"mode":"connected","gateway_url":gateway})
                .to_string();
            assert!(parse_configuration(&input).is_ok(), "{gateway}");
        }
    }

    #[test]
    fn bad_config_is_an_error_instead_of_silent_preview_fallback() {
        for input in [
            "{}",
            r#"{"version":1,"mode":"connected","gateway_url":null}"#,
            r#"{"version":2,"mode":"preview","gateway_url":null}"#,
            r#"{"version":1,"mode":"preview","gateway_url":"https://gateway.example"}"#,
            r#"{"version":1,"mode":"mystery","gateway_url":null}"#,
        ] {
            assert!(parse_configuration(input).is_err());
        }
    }

    #[test]
    fn credentials_unsafe_schemes_and_url_tokens_are_rejected_without_echoing() {
        for gateway in [
            "http://public.example",
            "https://name:secret@gateway.example",
            "https://gateway.example/?token=secret",
            "https://gateway.example/#secret",
            "//gateway.example",
            "/gateway?token=secret",
            "/gateway#secret",
            "javascript:secret",
            "data:secret",
            "https://gateway.example/secret\n",
            "",
        ] {
            let input = serde_json::json!({"version":1,"mode":"connected","gateway_url":gateway})
                .to_string();
            let error = parse_configuration(&input).unwrap_err();
            assert!(!error.contains("secret"));
        }
    }

    #[test]
    fn native_player_is_opt_in_and_cannot_run_in_preview_mode() {
        let legacy =
            parse_configuration(r#"{"version":1,"mode":"connected","gateway_url":"/gateway"}"#)
                .unwrap();
        assert!(!legacy.native_lots);
        let native = parse_configuration(
            r#"{"version":1,"mode":"connected","gateway_url":"/gateway","native_lots":true}"#,
        )
        .unwrap();
        assert!(native.native_lots);
        assert!(
            parse_configuration(
                r#"{"version":1,"mode":"preview","gateway_url":null,"native_lots":true}"#
            )
            .is_err()
        );
        assert!(
            parse_configuration(
                r#"{"version":1,"mode":"connected","gateway_url":"/gateway","native_lots":"true"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn oversized_configuration_is_rejected() {
        assert!(parse_configuration(&" ".repeat(MAX_CONFIG_BYTES + 1)).is_err());
    }
}
