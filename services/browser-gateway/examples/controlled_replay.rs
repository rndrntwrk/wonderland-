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
const LOT_LOCATION: u32 = (256 << 16) | 256;

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
            format!("<The-Sims-Online><Avatar-Data><AvatarID>42</AvatarID><Name>Controlled Alice</Name><Shard-Name>Controlled City</Shard-Name><Description>Synthetic browser replay account</Description><Head>{}</Head><Body>{}</Body><Appearance>Light</Appearance><LotId>1</LotId><LotName>Controlled Source Lot</LotName><LotLocation>{LOT_LOCATION}</LotLocation></Avatar-Data></The-Sims-Online>",0x0000_03a1_0000_000du64,0x0000_024a_0000_000du64).into_response()
        }))
        .route("/cityselector/shard-status.jsp",get(||async{
            "<Shard-Status-List><Shard-Status><Id>7</Id><Name>Controlled City</Name><Rank>1</Rank><Map>0100</Map><Status>Up</Status></Shard-Status></Shard-Status-List>"
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
    json!({"avatar_id":42,"shard_id":7,"name":"Controlled Alice","gender":"male","date":0,"description":"Synthetic browser replay account","current_job":0,"mayor_nhood":null})
}
fn lot() -> Value {
    json!({"lot_id":1,"location":LOT_LOCATION,"name":"Controlled Source Lot","description":"Test-only source-v38 avatar snapshot; refresh presentation only","shard_id":7,"owner_id":42,"roommates":[42],"neighborhood_id":99,"category":"residential","admit_mode":0,"skill_mode":0,"created_date":0})
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
            json!({"avatars_online_count":1,"avatars":[{"avatar_id":42,"name":"Controlled Alice","privacy_mode":0,"location":LOT_LOCATION}]})
        }
        "/userapi/lots/1" => lot(),
        path if path == format!("/userapi/city/7/lots/location/{LOT_LOCATION}") => lot(),
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
                    found.extend(if location == LOT_LOCATION { 0u16 } else { 3u16 }.to_be_bytes());
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
                    body.extend(0u16.to_be_bytes());
                    body.extend(LOT_LOCATION.to_be_bytes());
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
fn tick_bytes(id: u32, commands: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![0]; // Broadcast tick list; not an immediate command.
    bytes.extend(1i32.to_le_bytes());
    bytes.extend(id.to_le_bytes());
    bytes.extend(99u64.to_le_bytes());
    bytes.extend((commands.len() as i32).to_le_bytes());
    for command in commands {
        bytes.extend(command);
    }
    bytes
}
fn snapshot_command() -> Vec<u8> {
    let mut command = vec![12]; // StateSync has no ActorUID prefix.
    command.extend(include_bytes!(
        "fixtures/test-only-synthetic-avatar-v38.fsov"
    ));
    command.push(0); // No trace list.
    command
}
fn source_string(value: &str) -> Vec<u8> {
    assert!(
        value.len() < 128,
        "Fixture strings use one .NET length byte"
    );
    [vec![value.len() as u8], value.as_bytes().to_vec()].concat()
}
fn greeting_commands() -> Vec<Vec<u8>> {
    // Original VMNetSimJoinCmd version 5 supplies the sender identity. These
    // synthetic bytes exercise the native observer, not injected browser events.
    let join = [
        vec![0],
        99u32.to_le_bytes().to_vec(),
        vec![0xee, 0xff],
        5i32.to_le_bytes().to_vec(),
        source_string("Controlled Bob"),
        99u32.to_le_bytes().to_vec(),
        vec![0; 24 + 1 + 8 + 16 + 6 + 86 + 12],
    ]
    .concat();
    let chat = [
        vec![4],
        99u32.to_le_bytes().to_vec(),
        source_string("Hello from the controlled lot."),
        vec![0],
    ]
    .concat();
    vec![join, chat]
}
async fn send_tick(stream: &mut TcpStream, bytes: &[u8]) -> ReplayResult<()> {
    let mut body = (bytes.len() as u32).to_be_bytes().to_vec();
    body.extend(bytes);
    stream.write_all(&encode_packet(1000, 7, &body)?).await?;
    Ok(())
}
async fn lot_peer(mut stream: TcpStream) -> ReplayResult<()> {
    handshake(&mut stream, LOT_TICKET).await?;
    let mut tick = 42;
    send_tick(&mut stream, &tick_bytes(tick, &[snapshot_command()])).await?;
    // Source SendState tags its snapshot with the NEXT ordinary tick ID.
    send_tick(&mut stream, &tick_bytes(tick, &greeting_commands())).await?;
    loop {
        for Packet {
            channel,
            packet_type,
            body,
        } in native::read_packets(&mut stream).await?
        {
            if channel == 1000 && packet_type == 9 && body.get(4) == Some(&13) {
                tick += 1;
                send_tick(&mut stream, &tick_bytes(tick, &[snapshot_command()])).await?;
            }
            // Other commands deliberately make no fixture mutation or fake receipt.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wonderland_browser_gateway::lot_chat::LotChatGate;
    use wonderland_vm_protocol::{CommandBody, decode_tick_list, snapshot::Appearance};

    #[test]
    fn controlled_browser_snapshot_has_an_interior_avatar_and_distinct_source_needs() {
        let bytes = tick_bytes(42, &[snapshot_command()]);
        let list = decode_tick_list(&bytes, &Default::default()).unwrap();
        assert_eq!(list.consumed, bytes.len());
        let CommandBody::StateSync { snapshot, .. } = &list.ticks[0].commands[0].body else {
            panic!("The browser fixture must start with an original StateSync");
        };
        assert_eq!(snapshot.platform.lot_id, LOT_LOCATION);
        assert_eq!(snapshot.context.architecture.width, 8);
        assert_eq!(snapshot.context.architecture.height, 8);
        let entity = &snapshot.entities[0];
        assert_eq!(entity.persist_id, 42);
        assert_eq!(entity.position.x, 64);
        assert_eq!(entity.position.y, 64);
        let Appearance::Avatar(avatar) = &entity.appearance else {
            panic!("The browser fixture must contain its account avatar");
        };
        assert_eq!(avatar.motives[5], 0);
        assert_eq!(avatar.motives[7], 80);
        assert_eq!(avatar.motives[13], -60);
    }

    #[test]
    fn controlled_greeting_is_projected_from_original_native_commands() {
        let mut gate = LotChatGate::new(42, LOT_LOCATION, 1);
        gate.observe(false, &tick_bytes(42, &[snapshot_command()]))
            .unwrap();
        let delivery = gate
            .observe(false, &tick_bytes(42, &greeting_commands()))
            .unwrap()
            .unwrap();
        let message = delivery
            .projection
            .messages
            .iter()
            .find(|message| message.kind == wonderland_vm_protocol::chat::SourceChatKind::Message)
            .expect("The native greeting must reach the visible chat projection");
        assert_eq!(message.sender_uid, 99);
        assert_eq!(message.sender_name, "Controlled Bob");
        assert_eq!(message.text, "Hello from the controlled lot.");
    }
}
