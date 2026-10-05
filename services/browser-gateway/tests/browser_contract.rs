use axum::{
    Router,
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use wonderland_browser_gateway::{app, config::GatewayConfig};
use wonderland_game_services::*;

async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    (
        format!("http://{address}"),
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }),
    )
}

async fn setup() -> (String, Vec<tokio::task::JoinHandle<()>>) {
    let (upstream, task) = serve(
        Router::new()
            .route(
                "/userapi/oauth/token",
                post(|| async { "{\"access_token\":\"source-secret-token\",\"expires_in\":3600}" }),
            )
            .route(
                "/cityselector/app/AvatarDataServlet",
                get(|| async { "<The-Sims-Online/>" }),
            )
            .route(
                "/cityselector/shard-status.jsp",
                get(|| async { "<Shard-Status-List/>" }),
            ),
    )
    .await;
    let (gateway, gateway_task) = serve(
        app(GatewayConfig {
            api_base_url: Some(upstream),
            allowed_origins: vec!["https://game.example".into()],
            ..Default::default()
        })
        .unwrap(),
    )
    .await;
    (gateway, vec![task, gateway_task])
}

#[tokio::test]
async fn health_is_honest_without_operator_service_configuration() {
    let (url, task) = serve(app(GatewayConfig::default()).unwrap()).await;
    let client = reqwest::Client::new();
    let result = client.get(format!("{url}/health")).send().await.unwrap();
    assert_eq!(result.status(), 200);
    let value: GatewayHealth = result.json().await.unwrap();
    assert!(!value.configured);
    let response = client
        .post(format!("{url}/v1/sessions"))
        .json(&LoginRequest {
            username: "a".into(),
            password: "b".into(),
        })
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    task.abort();
}

#[tokio::test]
async fn browser_sessions_keep_credentials_out_of_urls_and_revoke_after_logout() {
    let (url, tasks) = setup().await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{url}/v1/sessions"))
        .json(&LoginRequest {
            username: "a".into(),
            password: "b".into(),
        })
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let created: SessionCreated = response.json().await.unwrap();
    let session = client
        .get(format!("{url}/v1/session"))
        .bearer_auth(&created.session_token)
        .send()
        .await
        .unwrap();
    assert_eq!(session.status(), 200);
    let revoked = client
        .delete(format!("{url}/v1/session"))
        .bearer_auth(&created.session_token)
        .send()
        .await
        .unwrap();
    assert!(revoked.status().is_success());
    assert_eq!(
        client
            .get(format!("{url}/v1/session"))
            .bearer_auth(&created.session_token)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    for t in tasks {
        t.abort();
    }
}

#[tokio::test]
async fn cross_origin_and_url_token_requests_are_rejected_before_session_work() {
    let (url, tasks) = setup().await;
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .post(format!("{url}/v1/sessions"))
            .header("origin", "https://evil.example")
            .json(&LoginRequest {
                username: "a".into(),
                password: "b".into()
            })
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .get(format!("{url}/v1/session?session_token=secret"))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let response = client
        .get(format!("{url}/health"))
        .header("origin", "https://game.example")
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "https://game.example"
    );
    for t in tasks {
        t.abort();
    }
}

#[tokio::test]
async fn socket_requires_auth_and_does_not_share_claims_between_sessions() {
    let (url, tasks) = setup().await;
    let client = reqwest::Client::new();
    let created: SessionCreated = client
        .post(format!("{url}/v1/sessions"))
        .json(&LoginRequest {
            username: "a".into(),
            password: "b".into(),
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ws_url = format!("{}/v1/ws", url.replacen("http:", "ws:", 1));
    let (mut ws, _) = connect_async(&ws_url).await.unwrap();
    ws.send(Message::Text(
        serde_json::to_string(&BrowserMessage::Authenticate {
            session_token: created.session_token.clone(),
        })
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();
    let message = ws.next().await.unwrap().unwrap().into_text().unwrap();
    let envelope: GatewayEnvelope = serde_json::from_str(&message).unwrap();
    assert!(matches!(envelope.event, GatewayEvent::Session { .. }));
    ws.send(Message::Text(
        serde_json::to_string(&BrowserMessage::Request {
            epoch: envelope.epoch - 1,
            operation_id: "old".into(),
            operation: GatewayOperation::RetireAvatar,
        })
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();
    let event: GatewayEnvelope =
        serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap()).unwrap();
    assert!(matches!(
        event.event,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Rejected,
            error: Some(ServiceError {
                code: ErrorCode::StaleEpoch,
                ..
            }),
            ..
        }
    ));
    let (mut duplicate, _) = connect_async(&ws_url).await.unwrap();
    duplicate
        .send(Message::Text(
            serde_json::to_string(&BrowserMessage::Authenticate {
                session_token: created.session_token.clone(),
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
    let event: GatewayEnvelope = serde_json::from_str(
        &duplicate
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        event.event,
        GatewayEvent::Error {
            error: ServiceError {
                code: ErrorCode::SessionConflict,
                ..
            }
        }
    ));
    client
        .delete(format!("{url}/v1/session"))
        .bearer_auth(&created.session_token)
        .send()
        .await
        .unwrap();
    let closed = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next()).await;
    assert!(closed.is_ok());
    for t in tasks {
        t.abort();
    }
}
