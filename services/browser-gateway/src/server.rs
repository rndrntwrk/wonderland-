use crate::{config::GatewayConfig, upstream::UpstreamClient};
use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, RwLock, Semaphore, watch};
use wonderland_game_services::*;

pub(crate) struct Shared {
    pub config: GatewayConfig,
    pub upstream: Option<UpstreamClient>,
    pub sessions: Mutex<HashMap<String, Arc<Session>>>,
    pub http_slots: Arc<Semaphore>,
    pub ws_slots: Arc<Semaphore>,
}
pub(crate) struct Session {
    pub grant: OAuthGrant,
    pub expires: tokio::time::Instant,
    pub projection: Mutex<SessionProjection>,
    pub roster: RwLock<Vec<RosterEntry>>,
    pub socket_claim: AtomicBool,
    pub ended: watch::Sender<bool>,
}

pub fn app(config: GatewayConfig) -> ServiceResult<Router> {
    if config.max_sessions == 0 || config.max_sessions > 10000 {
        return Err(ServiceError::new(
            ErrorCode::InvalidRequest,
            "Invalid gateway session resource quota",
        ));
    }
    let upstream = config
        .api_base_url
        .as_deref()
        .map(UpstreamClient::new)
        .transpose()?;
    let state = Arc::new(Shared {
        ws_slots: Arc::new(Semaphore::new(config.max_sessions)),
        config,
        upstream,
        sessions: Mutex::new(HashMap::new()),
        http_slots: Arc::new(Semaphore::new(32)),
    });
    Ok(Router::new()
        .route("/health", get(health))
        .route("/v1/sessions", post(login))
        .route("/v1/session", get(session).delete(logout))
        .route("/v1/roster", get(roster))
        .route("/v1/shards", get(shards))
        .route("/v1/query", post(query))
        .route("/v1/ws", get(websocket))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), boundary))
        .with_state(state))
}

pub(crate) struct HttpError(pub ServiceError);
impl From<ServiceError> for HttpError {
    fn from(value: ServiceError) -> Self {
        Self(value)
    }
}
impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status = match self.0.code {
            ErrorCode::Unauthorized
            | ErrorCode::AuthenticationFailed
            | ErrorCode::AccountLocked => StatusCode::UNAUTHORIZED,
            ErrorCode::NotConfigured | ErrorCode::Transport => StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::ResourceBusy => StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::SessionConflict | ErrorCode::OperationPending => StatusCode::CONFLICT,
            ErrorCode::DestinationRejected => StatusCode::FORBIDDEN,
            ErrorCode::ResponseTooLarge => StatusCode::BAD_GATEWAY,
            _ => StatusCode::BAD_REQUEST,
        };
        (status, Json(self.0)).into_response()
    }
}

async fn boundary(State(state): State<Arc<Shared>>, request: Request, next: Next) -> Response {
    let origin = request.headers().get(header::ORIGIN).cloned();
    let allowed = origin.as_ref().is_none_or(|origin| {
        origin
            .to_str()
            .ok()
            .is_some_and(|s| state.config.allowed_origins.iter().any(|value| value == s))
    });
    let mut response = if !allowed {
        (
            StatusCode::FORBIDDEN,
            Json(ServiceError::new(
                ErrorCode::Unauthorized,
                "This browser origin is not enabled for the gateway",
            )),
        )
            .into_response()
    } else if request.uri().query().is_some() {
        HttpError(ServiceError::new(
            ErrorCode::InvalidRequest,
            "Gateway credentials and operations must not be placed in URLs",
        ))
        .into_response()
    } else if request.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    if allowed && let Some(origin) = origin {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        response
            .headers_mut()
            .insert(header::VARY, header::HeaderValue::from_static("Origin"));
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            header::HeaderValue::from_static("GET, POST, DELETE, OPTIONS"),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            header::HeaderValue::from_static("Authorization, Content-Type"),
        );
    }
    response
}

fn upstream(state: &Shared) -> ServiceResult<&UpstreamClient> {
    state.upstream.as_ref().ok_or_else(|| {
        ServiceError::new(
            ErrorCode::NotConfigured,
            "An original account service has not been configured",
        )
    })
}
fn busy() -> ServiceError {
    ServiceError::new(
        ErrorCode::ResourceBusy,
        "Gateway connection resources are busy; try again later",
    )
}
fn unauthorized() -> ServiceError {
    ServiceError::new(
        ErrorCode::Unauthorized,
        "The gateway session is missing or expired",
    )
}

async fn health(State(state): State<Arc<Shared>>) -> Json<GatewayHealth> {
    Json(GatewayHealth {
        protocol_version: GATEWAY_PROTOCOL_VERSION,
        configured: state.upstream.is_some(),
        capabilities: capabilities(
            &SessionState::Disconnected,
            None,
            None,
            state.upstream.is_some(),
        ),
    })
}

async fn login(
    State(state): State<Arc<Shared>>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<SessionCreated>, HttpError> {
    let _permit = state
        .http_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| busy())?;
    let api = upstream(&state)?;
    let grant = api
        .authenticate(&request.username, &request.password)
        .await?;
    drop(request);
    let (roster, shards) = tokio::try_join!(api.roster(&grant.token), api.shards())?;
    let projection = SessionProjection {
        epoch: 1,
        state: SessionState::Authenticated,
        avatar_id: None,
        shard_name: None,
        lot_location: None,
        lot_incarnation: None,
        capabilities: capabilities(&SessionState::Authenticated, None, None, true),
    };
    let (ended, _) = watch::channel(false);
    let session = Arc::new(Session {
        expires: tokio::time::Instant::now()
            + Duration::from_secs(u64::from(grant.expires_in).min(86_400)),
        grant,
        projection: Mutex::new(projection.clone()),
        roster: RwLock::new(roster.clone()),
        socket_claim: AtomicBool::new(false),
        ended,
    });
    let token = uuid::Uuid::new_v4().simple().to_string();
    let mut sessions = state.sessions.lock().await;
    sessions.retain(|_, s| {
        if s.expires <= tokio::time::Instant::now() {
            s.ended.send_replace(true);
            false
        } else {
            true
        }
    });
    if sessions.len() >= state.config.max_sessions {
        return Err(busy().into());
    }
    sessions.insert(token.clone(), session);
    Ok(Json(SessionCreated {
        session_token: token,
        session: projection,
        roster,
        shards,
    }))
}

fn bearer(headers: &HeaderMap) -> ServiceResult<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(unauthorized)
}
pub(crate) async fn lookup(state: &Shared, token: &str) -> ServiceResult<Arc<Session>> {
    if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(unauthorized());
    }
    let mut sessions = state.sessions.lock().await;
    let session = sessions.get(token).cloned().ok_or_else(unauthorized)?;
    if session.expires <= tokio::time::Instant::now() || *session.ended.borrow() {
        sessions.remove(token);
        session.ended.send_replace(true);
        return Err(unauthorized());
    }
    Ok(session)
}
async fn session(
    State(state): State<Arc<Shared>>,
    headers: HeaderMap,
) -> Result<Json<SessionProjection>, HttpError> {
    let session = lookup(&state, bearer(&headers)?).await?;
    Ok(Json(session.projection.lock().await.clone()))
}
async fn logout(
    State(state): State<Arc<Shared>>,
    headers: HeaderMap,
) -> Result<StatusCode, HttpError> {
    let token = bearer(&headers)?;
    if let Some(session) = state.sessions.lock().await.remove(token) {
        session.ended.send_replace(true);
    }
    Ok(StatusCode::NO_CONTENT)
}
async fn roster(
    State(state): State<Arc<Shared>>,
    headers: HeaderMap,
) -> Result<Json<Vec<RosterEntry>>, HttpError> {
    let _permit = state
        .http_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| busy())?;
    let session = lookup(&state, bearer(&headers)?).await?;
    let roster = upstream(&state)?.roster(&session.grant.token).await?;
    if *session.ended.borrow() {
        return Err(unauthorized().into());
    }
    *session.roster.write().await = roster.clone();
    Ok(Json(roster))
}
async fn shards(State(state): State<Arc<Shared>>) -> Result<Json<Vec<Shard>>, HttpError> {
    let _permit = state
        .http_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| busy())?;
    Ok(Json(upstream(&state)?.shards().await?))
}
async fn query(
    State(state): State<Arc<Shared>>,
    Json(request): Json<DirectoryRequest>,
) -> Result<Json<DirectoryResult>, HttpError> {
    let _permit = state
        .http_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| busy())?;
    Ok(Json(upstream(&state)?.query(request.query).await?))
}

async fn websocket(
    State(state): State<Arc<Shared>>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, HttpError> {
    let permit = state
        .ws_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| busy())?;
    Ok(upgrade
        .max_message_size(128 * 1024)
        .max_frame_size(128 * 1024)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            socket_start(socket, state).await;
        }))
}
async fn socket_start(mut socket: WebSocket, state: Arc<Shared>) {
    let authentication = tokio::time::timeout(Duration::from_secs(5), socket.recv()).await;
    let message = match authentication {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<BrowserMessage>(&text).ok(),
        _ => None,
    };
    let token = match message {
        Some(BrowserMessage::Authenticate { session_token }) => session_token,
        _ => {
            socket_error(&mut socket, unauthorized()).await;
            return;
        }
    };
    let session = match lookup(&state, &token).await {
        Ok(session) => session,
        Err(error) => {
            socket_error(&mut socket, error).await;
            return;
        }
    };
    drop(token);
    if session
        .socket_claim
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        socket_error(
            &mut socket,
            ServiceError::new(
                ErrorCode::SessionConflict,
                "This account session already has an active browser connection",
            ),
        )
        .await;
        return;
    }
    {
        let mut projection = session.projection.lock().await;
        projection.epoch += 1;
        projection.state = SessionState::Authenticated;
        projection.avatar_id = None;
        projection.shard_name = None;
        projection.lot_location = None;
        projection.lot_incarnation = None;
        projection.capabilities = capabilities(&projection.state, None, None, true);
    }
    crate::actor::run(socket, state, session.clone()).await;
    session.socket_claim.store(false, Ordering::Release);
}
async fn socket_error(socket: &mut WebSocket, error: ServiceError) {
    let event = GatewayEnvelope {
        epoch: 0,
        operation_id: None,
        event: GatewayEvent::Error { error },
    };
    if let Ok(text) = serde_json::to_string(&event) {
        let _ = socket.send(Message::Text(text.into())).await;
    }
    let _ = socket.send(Message::Close(None)).await;
}

pub(crate) fn capabilities(
    state: &SessionState,
    avatar: Option<u32>,
    eod: Option<u32>,
    configured: bool,
) -> Vec<CapabilityStatus> {
    let city = matches!(
        state,
        SessionState::CityReady | SessionState::LotConnecting | SessionState::LotReady
    );
    let actor = city && avatar.is_some_and(|a| a != 0);
    let lot = *state == SessionState::LotReady && actor;
    let pairs = [
        ("account", configured),
        ("directory", configured),
        ("city", configured),
        ("lot", actor),
        ("create_avatar", city && avatar == Some(0)),
        ("retire_avatar", actor),
        ("private_message", actor),
        ("mail", actor),
        ("property", actor),
        ("neighborhood", actor),
        ("bulletin", actor),
        ("lot_chat", lot),
        ("lot_command", lot),
        ("world_snapshot", lot),
        ("cancel_interaction", lot),
        ("walk", lot),
        ("eod", eod.is_some()),
        (
            "wardrobe",
            matches!(eod, Some(0x8b300068 | 0xcb492685 | 0x2b58020b)),
        ),
        ("live_world", false),
        ("live_hud", false),
        ("profile_edit", false),
        ("bookmarks", false),
        ("inventory", false),
    ];
    pairs
        .into_iter()
        .map(|(name, available)| CapabilityStatus {
            capability: name.into(),
            available,
            reason: if available {
                None
            } else {
                Some(
                    match name {
                        "live_world" | "live_hud" => "Original VM world projection is not attached",
                        "profile_edit" | "bookmarks" => {
                            "Original data-service model adapter is not attached"
                        }
                        "eod" | "wardrobe" => "Requires a server-observed active EOD session",
                        "inventory" | "lot_command" => {
                            "Requires the original command/state adapter"
                        }
                        _ => "Requires the corresponding authenticated session",
                    }
                    .into(),
                )
            },
        })
        .collect()
}
