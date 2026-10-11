use wonderland_game_services::*;

#[test]
fn routes_emit_original_endpoint_and_escape_user_values() {
    let api = ApiRoutes::new("https://api.example/").unwrap();
    assert_eq!(
        api.oauth().as_str(),
        "https://api.example/userapi/oauth/token"
    );
    assert_eq!(api.roster().path(), "/cityselector/app/AvatarDataServlet");
    assert_eq!(
        api.selection("City & One", 42).unwrap().as_str(),
        "https://api.example/cityselector/app/ShardSelectorServlet?shardName=City+%26+One&avatarId=42"
    );
    let query = DirectoryQuery::AvatarSearch {
        shard_id: 8,
        name: "Alice/Bob?secret".into(),
    };
    assert_eq!(
        api.query(&query).unwrap().as_str(),
        "https://api.example/userapi/city/8/avatars/name/Alice%2FBob%3Fsecret"
    );
    assert_eq!(
        oauth_form("a+b", "p&x").unwrap(),
        "username=a%2Bb&password=p%26x&permission_level=1"
    );
}

#[test]
fn no_arbitrary_url_credentials_or_zero_based_source_pages() {
    for url in [
        "file:///secret",
        "https://user:password@api.example/",
        "https://api.example/?token=x",
        "https://api.example/#password",
    ] {
        assert!(ApiRoutes::new(url).is_err());
    }
    let api = ApiRoutes::new("https://api.example/").unwrap();
    assert!(
        api.query(&DirectoryQuery::LotPage {
            shard_id: 1,
            page: 0,
            per_page: 100
        })
        .is_err()
    );
    assert!(
        api.query(&DirectoryQuery::AvatarPage {
            shard_id: 1,
            page: 1,
            per_page: 501
        })
        .is_err()
    );
    assert_eq!(
        api.query(&DirectoryQuery::AvatarPage {
            shard_id: 1,
            page: 2,
            per_page: 500
        })
        .unwrap()
        .as_str(),
        "https://api.example/userapi/city/1/avatars/page/2?avatars_on_page=500"
    );
}
