use wonderland_game_services::*;

#[test]
fn oauth_rejects_source_http_200_error_without_exposing_body() {
    let err = parse_oauth(200, br#"{"error":"unauthorized_client","error_description":"account_locked","password":"secret"}"#).unwrap_err();
    assert_eq!(err.code, ErrorCode::AccountLocked);
    assert!(!format!("{err:?}").contains("secret"));
}

#[test]
fn oauth_rejects_non_success_http_even_if_body_contains_token() {
    let err = parse_oauth(
        401,
        br#"{"access_token":"private-token","expires_in":3600}"#,
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::Unauthorized);
    assert!(!format!("{err:?}").contains("private-token"));
}

#[test]
fn original_roster_preserves_u64_keys_and_absent_live_values() {
    let roster = parse_roster(200, br#"<The-Sims-Online><Avatar-Data><AvatarID>40</AvatarID><Name>Alice &amp; Bob</Name><Shard-Name>City One</Shard-Name><Head>18446744073709551615</Head><Body>9007199254740993</Body><Appearance>Light</Appearance><Description>hello</Description></Avatar-Data></The-Sims-Online>"#).unwrap();
    assert_eq!(roster[0].head_key, Some(DecimalU64(u64::MAX)));
    assert_eq!(roster[0].name, "Alice & Bob");
    assert!(roster[0].money.is_none());
    assert!(roster[0].motives.is_none());
    assert!(roster[0].home.is_none());
    let json = serde_json::to_value(&roster[0]).unwrap();
    assert_eq!(json["body_key"], "9007199254740993");
}

#[test]
fn malformed_nested_trailing_and_doctype_xml_fail_closed() {
    for xml in [
        "<The-Sims-Online>",
        "<wrong/>",
        "<The-Sims-Online/><The-Sims-Online/>",
        "<!DOCTYPE a [<!ENTITY x 'secret'>]><The-Sims-Online/>",
        "<The-Sims-Online><Avatar-Data><AvatarID>-1</AvatarID></Avatar-Data></The-Sims-Online>",
    ] {
        assert_eq!(
            parse_roster(200, xml.as_bytes()).unwrap_err().code,
            ErrorCode::InvalidResponse
        );
    }
}

#[test]
fn selection_reads_authenticated_ticket_and_source_xml_error() {
    let selection = parse_city_selection(200, br#"<Shard-Selection><Connection-Address>city.example:</Connection-Address><Authorization-Ticket>12345678901234567890123456789012</Authorization-Ticket><PlayerID>9</PlayerID><ConnectionID>secret</ConnectionID><AvatarID>40</AvatarID></Shard-Selection>"#).unwrap();
    assert_eq!(selection.avatar_id, 40);
    assert!(!format!("{selection:?}").contains("12345678901234567890123456789012"));
    let err = parse_city_selection(200, br#"<Error-Message><Error-Number>505</Error-Number><Error>You do not own this avatar!</Error></Error-Message>"#).unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn shard_list_uses_original_names_and_ids() {
    let shards = parse_shards(200, br#"<Shard-Status-List><Shard-Status><Location>public</Location><Name>City One</Name><Rank>1</Rank><Map>0001</Map><Status>Up</Status><Id>7</Id></Shard-Status></Shard-Status-List>"#).unwrap();
    assert_eq!(shards[0].id, 7);
    assert_eq!(shards[0].name, "City One");
}
