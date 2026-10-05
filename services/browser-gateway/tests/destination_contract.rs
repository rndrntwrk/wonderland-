use std::{collections::BTreeMap, net::SocketAddr};
use wonderland_browser_gateway::config::DestinationAllowlist;
use wonderland_game_services::ErrorCode;

#[test]
fn source_101_suffix_resolves_only_to_operator_pinned_endpoints() {
    let policy = DestinationAllowlist::from_pinned(BTreeMap::from([(
        "city.example:2101".into(),
        vec!["127.0.0.1:32101".parse::<SocketAddr>().unwrap()],
    )]))
    .unwrap();
    assert_eq!(
        policy.selected("city.example:2").unwrap(),
        ["127.0.0.1:32101".parse::<SocketAddr>().unwrap()]
    );
    for destination in [
        "evil.example:",
        "127.0.0.1:2",
        "city.example:2101",
        "http://city.example:2",
        "city.example:2@evil.example:",
        "city.example:2/path",
    ] {
        assert_eq!(
            policy.selected(destination).unwrap_err().code,
            ErrorCode::DestinationRejected
        );
    }
}

#[test]
fn empty_allowlist_rejects_even_authenticated_addresses() {
    assert_eq!(
        DestinationAllowlist::default()
            .selected("localhost:")
            .unwrap_err()
            .code,
        ErrorCode::DestinationRejected
    );
}
