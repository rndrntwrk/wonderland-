use axum::{
    Router,
    extract::{Form, Request},
    http::StatusCode,
    routing::{get, post},
};
use std::collections::HashMap;
use wonderland_browser_gateway::upstream::UpstreamClient;
use wonderland_game_services::*;

async fn start(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{address}"), task)
}

#[test]
fn embedding_cannot_send_account_credentials_over_remote_plain_http() {
    assert!(UpstreamClient::new("http://accounts.example/").is_err());
    assert!(
        wonderland_browser_gateway::app(wonderland_browser_gateway::config::GatewayConfig {
            api_base_url: Some("http://accounts.example/".into()),
            ..Default::default()
        })
        .is_err()
    );
    for base in [
        "http://127.0.0.1:9010/",
        "http://[::1]:9010/",
        "http://localhost:9010/",
        "https://accounts.example/",
    ] {
        assert!(UpstreamClient::new(base).is_ok());
    }
}

#[tokio::test]
async fn original_http_form_and_bearer_roster_are_used_end_to_end() {
    let router = Router::new()
        .route(
            "/userapi/oauth/token",
            post(|Form(form): Form<HashMap<String, String>>| async move {
                assert_eq!(form.get("username").map(String::as_str), Some("a+b"));
                assert_eq!(form.get("password").map(String::as_str), Some("p&x"));
                assert_eq!(form.get("permission_level").map(String::as_str), Some("1"));
                "{\"access_token\":\"private-test-token\",\"expires_in\":3600}"
            }),
        )
        .route(
            "/cityselector/app/AvatarDataServlet",
            get(|request: Request| async move {
                assert!(request.uri().query().is_none());
                assert_eq!(
                    request.headers()["authorization"],
                    "Bearer private-test-token"
                );
                assert!(!request.headers().contains_key("cookie"));
                "<The-Sims-Online/>"
            }),
        );
    let (url, task) = start(router).await;
    let client = UpstreamClient::new(&url).unwrap();
    let grant = client.authenticate("a+b", "p&x").await.unwrap();
    assert!(client.roster(&grant.token).await.unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn source_oauth_http200_error_and_redirect_do_not_create_a_grant() {
    let router = Router::new().route(
        "/userapi/oauth/token",
        post(|| async {
            "{\"error\":\"unauthorized_client\",\"error_description\":\"user_credentials_invalid\"}"
        }),
    );
    let (url, task) = start(router).await;
    assert_eq!(
        UpstreamClient::new(&url)
            .unwrap()
            .authenticate("a", "b")
            .await
            .unwrap_err()
            .code,
        ErrorCode::AuthenticationFailed
    );
    task.abort();
    let received = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let received_by_target = received.clone();
    let (target, target_task) = start(Router::new().route(
        "/redirect-target",
        post(move || {
            let received = received_by_target.clone();
            async move {
                received.store(true, std::sync::atomic::Ordering::Release);
                "{\"access_token\":\"redirect-must-not-receive-credentials\",\"expires_in\":3600}"
            }
        }),
    ))
    .await;
    let router = Router::new().route(
        "/userapi/oauth/token",
        post(move || {
            let target = target.clone();
            async move {
                (
                    StatusCode::TEMPORARY_REDIRECT,
                    [("location", format!("{target}/redirect-target"))],
                    "",
                )
            }
        }),
    );
    let (url, task) = start(router).await;
    assert_eq!(
        UpstreamClient::new(&url)
            .unwrap()
            .authenticate("a", "b")
            .await
            .unwrap_err()
            .code,
        ErrorCode::Transport
    );
    assert!(!received.load(std::sync::atomic::Ordering::Acquire));
    target_task.abort();
    task.abort();
}
