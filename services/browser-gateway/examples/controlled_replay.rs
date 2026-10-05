//! Explicit local test harness. Never used by production startup or as a fallback.
//! Fixed synthetic account: username `controlled-player`, password `test-only`.
//! Synthetic source-wire state, not a deployed city or deterministic simulation.
use axum::{
    Json, Router,
    extract::{Form, Query, Request},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};
use wonderland_browser_gateway::{
    app,
    config::{DestinationAllowlist, GatewayConfig},
    native,
};
use wonderland_game_services::protocol::{Packet, encode_packet};

type ReplayResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const TOKEN: &str = "controlled-account-token";
const CITY_TICKET: &[u8] = b"11111111111111111111111111111111";
const LOT_TICKET: &[u8] = b"22222222222222222222222222222222";

#[tokio::main]
async fn main() -> ReplayResult<()> {
    let bind: SocketAddr = std::env::var("WONDERLAND_REPLAY_BIND")
        .unwrap_or_else(|_| "127.0.0.1:18787".into())
        .parse()?;
    if !bind.ip().is_loopback() {
        return Err("Controlled replay only binds loopback".into());
    }
    let origins = std::env::var("WONDERLAND_REPLAY_BROWSER_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let api = TcpListener::bind("127.0.0.1:0").await?;
    let city = TcpListener::bind("127.0.0.1:0").await?;
    let lot = TcpListener::bind("127.0.0.1:0").await?;
    let config = GatewayConfig {
        bind,
        api_base_url: Some(format!("http://{}", api.local_addr()?)),
        allowed_origins: origins,
        destinations: DestinationAllowlist::from_pinned(BTreeMap::from([
            (
                "controlled-city.example:101".into(),
                vec![city.local_addr()?],
            ),
            ("controlled-lot.example:101".into(), vec![lot.local_addr()?]),
        ]))?,
        max_sessions: 8,
    };
    tokio::spawn(async move {
        let _ = axum::serve(api, upstream()).await;
    });
    tokio::spawn(async move {
        while let Ok((stream, _)) = city.accept().await {
            tokio::spawn(async move {
                let _ = city_peer(stream).await;
            });
        }
    });
    tokio::spawn(async move {
        while let Ok((stream, _)) = lot.accept().await {
            tokio::spawn(async move {
                let _ = lot_peer(stream).await;
            });
        }
    });
    let gateway = TcpListener::bind(bind).await?;
    eprintln!(
        "TEST ONLY: controlled original-wire replay at http://{}",
        gateway.local_addr()?
    );
    eprintln!(
        "Synthetic account and source-v38 snapshot; no external services or game simulation."
    );
    axum::serve(gateway, app(config)?)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn upstream() -> Router {
    Router::new()
        .route("/userapi/oauth/token",post(|Form(form):Form<HashMap<String,String>>|async move {
            if form.get("username").map(String::as_str)==Some("controlled-player")
                && form.get("password").map(String::as_str)==Some("test-only")
                && form.get("permission_level").map(String::as_str)==Some("1") {
                Json(json!({"access_token":TOKEN,"expires_in":3600}))
            } else {Json(json!({"error":"unauthorized_client","error_description":"user_credentials_invalid"}))}
        }))
        .route("/cityselector/app/AvatarDataServlet",get(|headers:HeaderMap|async move {
            if !authenticated(&headers){return StatusCode::UNAUTHORIZED.into_response();}
            "<The-Sims-Online><Avatar-Data><AvatarID>42</AvatarID><Name>Controlled Alice</Name><Shard-Name>Controlled City</Shard-Name><Description>Synthetic browser replay account</Description><LotId>1</LotId><LotName>Controlled Source Lot</LotName><LotLocation>55</LotLocation></Avatar-Data></The-Sims-Online>".into_response()
        }))
        .route("/cityselector/shard-status.jsp",get(||async{
            "<Shard-Status-List><Shard-Status><Id>7</Id><Name>Controlled City</Name><Rank>1</Rank><Map>0001</Map><Status>Up</Status></Shard-Status></Shard-Status-List>"
        }))
        .route("/cityselector/app/ShardSelectorServlet",get(|headers:HeaderMap,Query(query):Query<HashMap<String,String>>|async move {
            if !authenticated(&headers){return StatusCode::UNAUTHORIZED.into_response();}
            if query.get("avatarId").map(String::as_str)!=Some("42") || query.get("shardName").map(String::as_str)!=Some("Controlled City") {
                return "<Error-Message><Error-Number>404</Error-Number><Error>Controlled avatar not found</Error></Error-Message>".into_response();
            }
            "<Shard-Selection><Connection-Address>controlled-city.example:</Connection-Address><Authorization-Ticket>11111111111111111111111111111111</Authorization-Ticket><PlayerID>9</PlayerID><AvatarID>42</AvatarID></Shard-Selection>".into_response()
        }))
        .fallback(directory)
}

fn authenticated(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        == Some("Bearer controlled-account-token")
}
fn avatar() -> Value {
    json!({"avatar_id":42,"shard_id":7,"name":"Controlled Alice","gender":"female","date":0,"description":"Synthetic browser replay account","current_job":0,"mayor_nhood":null})
}
fn lot() -> Value {
    json!({"lot_id":1,"location":55,"name":"Controlled Source Lot","description":"Synthetic golden source-v38 snapshot; refresh presentation only","shard_id":7,"owner_id":42,"roommates":[42],"neighborhood_id":99,"category":"residential","admit_mode":0,"skill_mode":0,"created_date":0})
}
fn neighborhood() -> Value {
    json!({"neighborhood_id":99,"name":"Controlled Neighborhood","description":"Test-only original directory result","color":3368601,"town_hall_id":null,"icon_url":null,"mayor_id":null,"mayor_elected_date":0,"election_cycle_id":null})
}
async fn directory(request: Request) -> Response {
    let path = request.uri().path();
    let result = match path {
        "/userapi/avatars/42" => avatar(),
        "/userapi/avatars" => json!({"avatars":[avatar()]}),
        "/userapi/avatars/online" => {
            json!({"avatars_online_count":1,"avatars":[{"avatar_id":42,"name":"Controlled Alice","privacy_mode":0,"location":55}]})
        }
        "/userapi/lots/1" | "/userapi/city/7/lots/location/55" => lot(),
        "/userapi/lots" | "/userapi/city/7/lots/neighborhood/99" => json!({"lots":[lot()]}),
        "/userapi/city/7/lots/online" => json!({"lots":[lot()]}),
        "/userapi/city/7/avatars/neighborhood/99" => json!({"avatars":[avatar()]}),
        "/userapi/city/7/neighborhoods/all" => json!({"neighborhoods":[neighborhood()]}),
        "/userapi/neighborhoods/99" => neighborhood(),
        "/userapi/neighborhood/99/elections" => json!({"elections":[]}),
        "/userapi/neighborhood/99/bulletins" => json!({"bulletins":[]}),
        path if path.starts_with("/userapi/city/7/lots/page/") => {
            let page = path
                .rsplit('/')
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(1);
            json!({"total_lots":1,"page":page,"total_pages":1,"lots_on_page":100,"lots":if page==1 {vec![lot()]} else {vec![]}})
        }
        path if path.starts_with("/userapi/city/7/avatars/page/") => {
            let page = path
                .rsplit('/')
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(1);
            json!({"total_avatars":1,"page":page,"total_pages":1,"avatars_on_page":100,"avatars":if page==1 {vec![avatar()]} else {vec![]}})
        }
        path if path.starts_with("/userapi/city/7/avatars/name/") => json!({"avatars":[avatar()]}),
        path if path.starts_with("/userapi/city/7/lots/name/") => json!({"lots":[lot()]}),
        path if path.starts_with("/userapi/neighborhood/99/bulletins/type/") => {
            json!({"bulletins":[]})
        }
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error":"No controlled fixture for this original route"})),
            )
                .into_response();
        }
    };
    Json(result).into_response()
}

async fn handshake(stream: &mut TcpStream, ticket: &[u8]) -> ReplayResult<()> {
    stream.write_all(&encode_packet(22, 22, &[])?).await?;
    let packets = native::read_packets(stream).await?;
    if packets.len() != 1
        || packets[0].channel != 21
        || packets[0].body.len() != 356
        || &packets[0].body[..3] != b"42\0"
        || &packets[0].body[324..] != ticket
    {
        return Err("Invalid controlled source handshake".into());
    }
    stream
        .write_all(&encode_packet(0, 30, &[0, 0, 0, 0, 16, 0])?)
        .await?;
    Ok(())
}
async fn city_peer(mut stream: TcpStream) -> ReplayResult<()> {
    handshake(&mut stream, CITY_TICKET).await?;
    loop {
        for packet in native::read_packets(&mut stream).await? {
            if packet.channel != 1000 {
                continue;
            }
            match packet.packet_type {
                5 => {
                    let location = u32::from_be_bytes(packet.body[..4].try_into()?);
                    let mut found = Vec::new();
                    found.extend(if location == 55 { 0u16 } else { 3u16 }.to_be_bytes());
                    found.extend(location.to_be_bytes());
                    found.push(32);
                    found.extend(LOT_TICKET);
                    found.push(23);
                    found.extend(b"controlled-lot.example:");
                    found.extend([2, b'4', b'2']);
                    stream.write_all(&encode_packet(1000, 6, &found)?).await?;
                }
                2 => {
                    stream
                        .write_all(&encode_packet(
                            1000,
                            3,
                            &[0, 2, 0, 3, 0, 0, 0, 0, 0, 0, 0, 99],
                        )?)
                        .await?
                }
                10 => {
                    let mut body = packet.body;
                    body.extend([0, 0, 0, 0, 0, 55]);
                    stream.write_all(&encode_packet(1000, 11, &body)?).await?;
                }
                12 => {
                    stream
                        .write_all(&encode_packet(1000, 14, &[0, 1, 0, 0, 0, 0])?)
                        .await?
                }
                18 => {
                    let kind = u16::from_be_bytes(packet.body[..2].try_into()?);
                    if kind != 2 {
                        stream
                            .write_all(&encode_packet(
                                1000,
                                19,
                                &[0, if kind == 0 { 0 } else { 6 }, 0, 0, 0, 0],
                            )?)
                            .await?;
                    }
                }
                20 => {
                    stream
                        .write_all(&encode_packet(1000, 21, &[0, 1, 0, 0, 0, 0, 0])?)
                        .await?
                }
                23 => {
                    stream
                        .write_all(&encode_packet(
                            1000,
                            24,
                            &[0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                        )?)
                        .await?
                }
                _ => {}
            }
        }
    }
}
async fn snapshot(stream: &mut TcpStream) -> ReplayResult<()> {
    let mut bytes =
        include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync-tick.bin")
            .to_vec();
    bytes[0] = 0;
    let mut body = (bytes.len() as u32).to_be_bytes().to_vec();
    body.extend(bytes);
    stream.write_all(&encode_packet(1000, 7, &body)?).await?;
    Ok(())
}
async fn lot_peer(mut stream: TcpStream) -> ReplayResult<()> {
    handshake(&mut stream, LOT_TICKET).await?;
    snapshot(&mut stream).await?;
    loop {
        for Packet {
            channel,
            packet_type,
            body,
        } in native::read_packets(&mut stream).await?
        {
            if channel == 1000 && packet_type == 9 && body.get(4) == Some(&13) {
                snapshot(&mut stream).await?;
            }
            // Other commands deliberately make no fixture mutation or fake receipt.
        }
    }
}
