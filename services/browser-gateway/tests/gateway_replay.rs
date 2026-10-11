//! Controlled source-wire replay. No deployed account or city is contacted.
use axum::{
    Router,
    extract::{Query, Request},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use wonderland_browser_gateway::{
    app,
    config::{DestinationAllowlist, GatewayConfig},
};
use wonderland_game_services::{protocol::*, *};

type BrowserSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    (
        format!("http://{address}"),
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }),
    )
}
async fn receive(stream: &mut TcpStream) -> Packet {
    loop {
        let mut header = [0; 12];
        stream.read_exact(&mut header).await.unwrap();
        let length = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        assert!(length < 65536);
        let mut bytes = header.to_vec();
        bytes.resize(12 + length, 0);
        stream.read_exact(&mut bytes[12..]).await.unwrap();
        let packet = decode_frame(&bytes, 65536).unwrap().1.remove(0);
        if packet.channel != 1000 || packet.packet_type != 13 {
            return packet;
        }
    }
}
async fn handshake(stream: &mut TcpStream, ticket: &[u8], city: bool) {
    stream
        .write_all(&[22, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        .await
        .unwrap();
    let response = receive(stream).await;
    assert_eq!(response.packet_type, 21);
    assert_eq!(&response.body[..3], b"42\0");
    assert_eq!(&response.body[324..], ticket);
    stream
        .write_all(&[
            0, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 30, 0, 0, 0, 12, 0, 0, 0, 0, 16, 0,
        ])
        .await
        .unwrap();
    assert_eq!(receive(stream).await.packet_type, 10);
    if city {
        assert_eq!(receive(stream).await.packet_type, 0x34);
        assert_eq!(receive(stream).await.packet_type, 0x36);
    }
}
async fn browser_event(ws: &mut BrowserSocket) -> GatewayEnvelope {
    let message = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(&message.into_text().unwrap()).unwrap()
}
async fn send(ws: &mut BrowserSocket, epoch: u64, id: &str, operation: GatewayOperation) {
    ws.send(Message::Text(
        serde_json::to_string(&BrowserMessage::Request {
            epoch,
            operation_id: id.into(),
            operation,
        })
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();
}
async fn outcome(
    ws: &mut BrowserSocket,
    id: &str,
    projection: &mut SessionProjection,
) -> GatewayEvent {
    loop {
        let event = browser_event(ws).await;
        if let GatewayEvent::Session { session } = event.event {
            *projection = session;
        } else if event.operation_id.as_deref() == Some(id)
            && matches!(event.event, GatewayEvent::Outcome { .. })
        {
            return event.event;
        }
    }
}
fn eod_tick() -> Vec<u8> {
    let mut tick = vec![0, 1, 0, 0, 0, 1, 0, 0, 0];
    tick.extend([0; 8]);
    tick.extend([1, 0, 0, 0, 18, 42, 0, 0, 0, 0x68, 0, 0x30, 0x8b, 9]);
    tick.extend(b"eod_enter");
    tick.extend([0, 0]);
    tick
}

#[tokio::test]
async fn browser_to_original_city_lot_replay_preserves_admission_rejection_and_eod_authority() {
    controlled_replay(55).await;
}

#[tokio::test]
async fn canceled_job_admission_drains_its_remapped_response_before_new_admission() {
    controlled_replay(0x200).await;
}

async fn controlled_replay(cancelled_location: u32) {
    let city = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let lot = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let policy = DestinationAllowlist::from_pinned(BTreeMap::from([
        ("city.example:101".into(), vec![city.local_addr().unwrap()]),
        ("lot.example:101".into(), vec![lot.local_addr().unwrap()]),
    ]))
    .unwrap();
    let (purchase_seen, seen) = oneshot::channel();
    let (release_purchase, purchase_release) = oneshot::channel();
    let (city_end, city_ended) = oneshot::channel();
    let (lot_end, lot_ended) = oneshot::channel();
    let (snapshot_seen, snapshot_read) = oneshot::channel();
    let (release_snapshot, snapshot_release) = oneshot::channel();
    let (second_snapshot_seen, second_snapshot_read) = oneshot::channel();
    let (cancelled_seen, cancelled_read) = oneshot::channel();
    let (release_cancelled, cancelled_release) = oneshot::channel();
    let city_task = tokio::spawn(async move {
        let (mut socket, _) = city.accept().await.unwrap();
        handshake(&mut socket, b"11111111111111111111111111111111", true).await;
        let purchase = receive(&mut socket).await;
        assert_eq!(purchase.packet_type, 2);
        assert_eq!(&purchase.body[..4], &[0, 8, 0, 9]);
        purchase_seen.send(()).unwrap();
        purchase_release.await.unwrap();
        socket
            .write_all(&encode_packet(1000, 3, &[0, 2, 0, 3, 0, 0, 0, 0, 0, 0, 0, 99]).unwrap())
            .await
            .unwrap();
        let neighborhood = receive(&mut socket).await;
        assert_eq!(neighborhood.packet_type, 20);
        assert_eq!(&neighborhood.body[..2], &[0, 8]);
        let mut candidates = vec![1, 0, 0, 0, 1, 0, 0, 0, 7, 4];
        candidates.extend(b"West");
        candidates.extend([255; 4]);
        socket
            .write_all(&encode_packet(1000, 22, &candidates).unwrap())
            .await
            .unwrap();
        socket
            .write_all(&encode_packet(1000, 21, &[0, 0, 0, 0, 0, 0, 0]).unwrap())
            .await
            .unwrap();
        let cancelled = receive(&mut socket).await;
        assert_eq!(cancelled.packet_type, 5);
        assert_eq!(&cancelled.body[..4], &cancelled_location.to_be_bytes());
        cancelled_seen.send(()).unwrap();
        cancelled_release.await.unwrap();
        let mut found = vec![0, 0, 0, 0, 0, 55, 32];
        found.extend(b"22222222222222222222222222222222");
        found.push(12);
        found.extend(b"lot.example:");
        found.extend([2, b'4', b'2']);
        socket
            .write_all(&encode_packet(1000, 6, &found).unwrap())
            .await
            .unwrap();
        for _ in 0..2 {
            let join = receive(&mut socket).await;
            assert_eq!(join.packet_type, 5);
            assert_eq!(join.body, [0, 0, 0, 55, 0]);
            let mut found = vec![0, 0, 0, 0, 0, 55, 32];
            found.extend(b"22222222222222222222222222222222");
            found.push(12);
            found.extend(b"lot.example:");
            found.extend([2, b'4', b'2']);
            socket
                .write_all(&encode_packet(1000, 6, &found).unwrap())
                .await
                .unwrap();
        }
        city_ended.await.unwrap();
    });
    let lot_task = tokio::spawn(async move {
        let (mut socket, _) = lot.accept().await.unwrap();
        handshake(&mut socket, b"22222222222222222222222222222222", false).await;
        socket
            .write_all(&encode_packet(1000, 7, &[0, 0, 0, 1, 255]).unwrap())
            .await
            .unwrap();
        socket
            .write_all(&encode_packet(1000, 7, &[0, 0, 0, 5, 0, 0, 0, 0, 0]).unwrap())
            .await
            .unwrap();
        let roof = receive(&mut socket).await;
        assert_eq!(roof.packet_type, 9);
        assert_eq!(
            roof.body,
            [0, 0, 0, 13, 32, 42, 0, 0, 0, 0, 0, 0, 63, 17, 0, 0, 0]
        );
        let tick = eod_tick();
        let mut body = (tick.len() as u32).to_be_bytes().to_vec();
        body.extend(tick);
        socket
            .write_all(&encode_packet(1000, 7, &body).unwrap())
            .await
            .unwrap();
        let eod = receive(&mut socket).await;
        assert_eq!(eod.packet_type, 9);
        assert_eq!(&eod.body[4..13], &[18, 42, 0, 0, 0, 0x68, 0, 0x30, 0x8b]);
        let walk = receive(&mut socket).await;
        assert_eq!(walk.packet_type, 9);
        assert_eq!(
            walk.body,
            [
                0, 0, 0, 14, 10, 42, 0, 0, 0, 25, 0, 249, 255, 160, 0, 64, 1, 2
            ]
        );
        let cancel = receive(&mut socket).await;
        assert_eq!(cancel.packet_type, 9);
        assert_eq!(cancel.body, [0, 0, 0, 7, 7, 42, 0, 0, 0, 5, 0]);
        let resync = receive(&mut socket).await;
        assert_eq!(resync.packet_type, 9);
        assert_eq!(resync.body, [0, 0, 0, 9, 13, 0, 0, 0, 0, 1, 0, 0, 0]);
        snapshot_seen.send(()).unwrap();
        snapshot_release.await.unwrap();
        let sync = include_bytes!(
            "../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync-tick.bin"
        );
        let mut body = (sync.len() as u32).to_be_bytes().to_vec();
        body.extend(sync);
        socket
            .write_all(&encode_packet(1000, 7, &body).unwrap())
            .await
            .unwrap();
        let (mut second, _) = lot.accept().await.unwrap();
        handshake(&mut second, b"22222222222222222222222222222222", false).await;
        second
            .write_all(&encode_packet(1000, 7, &[0, 0, 0, 5, 0, 0, 0, 0, 0]).unwrap())
            .await
            .unwrap();
        let resync = receive(&mut second).await;
        assert_eq!(resync.packet_type, 9);
        assert_eq!(resync.body, [0, 0, 0, 9, 13, 0, 0, 0, 0, 0, 0, 0, 0]);
        second_snapshot_seen.send(()).unwrap();
        lot_ended.await.unwrap();
    });
    let upstream=Router::new().route("/userapi/oauth/token",post(||async{"{\"access_token\":\"account-token\",\"expires_in\":3600}"}))
        .route("/cityselector/app/AvatarDataServlet",get(||async{"<The-Sims-Online><Avatar-Data><AvatarID>42</AvatarID><Name>Alice</Name><Shard-Name>City One</Shard-Name><Head>9007199254740993</Head><Body>9007199254740995</Body></Avatar-Data></The-Sims-Online>"}))
        .route("/cityselector/shard-status.jsp",get(||async{"<Shard-Status-List><Shard-Status><Id>7</Id><Name>City One</Name><Rank>1</Rank><Map>0001</Map><Status>Up</Status></Shard-Status></Shard-Status-List>"}))
        .route("/cityselector/app/ShardSelectorServlet",get(|Query(query):Query<HashMap<String,String>>,request:Request|async move {
            assert_eq!(request.headers()["authorization"],"Bearer account-token");assert_eq!(query["shardName"],"City One");assert_eq!(query["avatarId"],"42");
            "<Shard-Selection><Connection-Address>city.example:</Connection-Address><Authorization-Ticket>11111111111111111111111111111111</Authorization-Ticket><PlayerID>9</PlayerID><AvatarID>42</AvatarID></Shard-Selection>"
        }));
    let (api, api_task) = serve(upstream).await;
    let (url, gateway_task) = serve(
        app(GatewayConfig {
            api_base_url: Some(api),
            destinations: policy,
            ..Default::default()
        })
        .unwrap(),
    )
    .await;
    let client = reqwest::Client::new();
    let created: SessionCreated = client
        .post(format!("{url}/v1/sessions"))
        .json(&LoginRequest {
            username: "alice".into(),
            password: "test-only".into(),
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let (mut ws, _) = connect_async(format!("{}/v1/ws", url.replacen("http:", "ws:", 1)))
        .await
        .unwrap();
    ws.send(Message::Text(
        serde_json::to_string(&BrowserMessage::Authenticate {
            session_token: created.session_token.clone(),
        })
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();
    let first = browser_event(&mut ws).await;
    let mut projection = match first.event {
        GatewayEvent::Session { session } => session,
        _ => panic!("session required"),
    };
    let epoch = projection.epoch;
    send(
        &mut ws,
        epoch,
        "wrong-avatar",
        GatewayOperation::ConnectCity {
            shard_name: "City One".into(),
            avatar_id: 43,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "wrong-avatar", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Rejected,
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "city",
        GatewayOperation::ConnectCity {
            shard_name: "City One".into(),
            avatar_id: 42,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "city", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Accepted,
            ..
        }
    ));
    assert_eq!(projection.state, SessionState::CityReady);
    send(
        &mut ws,
        epoch,
        "purchase",
        GatewayOperation::PurchaseLot {
            x: 8,
            y: 9,
            name: "New Home".into(),
            start_fresh: false,
            mayor_mode: false,
        },
    )
    .await;
    assert!(matches!(
        browser_event(&mut ws).await.event,
        GatewayEvent::Pending { .. }
    ));
    seen.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(40), ws.next())
            .await
            .is_err(),
        "socket write must not produce acceptance"
    );
    release_purchase.send(()).unwrap();
    assert!(matches!(
        outcome(&mut ws, "purchase", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Rejected,
            source_code: Some(3),
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "purchase",
        GatewayOperation::PurchaseLot {
            x: 8,
            y: 9,
            name: "New Home".into(),
            start_fresh: false,
            mayor_mode: false,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "purchase", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Rejected,
            error: Some(ServiceError {
                code: ErrorCode::InvalidRequest,
                ..
            }),
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "candidates",
        GatewayOperation::Neighborhood {
            action: NeighborhoodAction::CanFreeVote,
            target_avatar_id: 0,
            neighborhood_id: 99,
            message: String::new(),
            value: 0,
        },
    )
    .await;
    loop {
        let event = browser_event(&mut ws).await;
        if let GatewayEvent::SourceEvent { family, data, .. } = event.event
            && family == "neighborhood_candidates"
        {
            assert_eq!(event.operation_id.as_deref(), Some("candidates"));
            assert_eq!(data["requested_neighborhood_id"], 99);
            break;
        }
    }
    assert!(matches!(
        outcome(&mut ws, "candidates", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Accepted,
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "cancelled-join",
        GatewayOperation::JoinLot {
            lot_location: cancelled_location,
            open_if_closed: false,
        },
    )
    .await;
    cancelled_read.await.unwrap();
    send(
        &mut ws,
        epoch,
        "cancel-admission",
        GatewayOperation::LeaveLot,
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "cancel-admission", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Accepted,
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "blocked-join",
        GatewayOperation::JoinLot {
            lot_location: cancelled_location,
            open_if_closed: false,
        },
    )
    .await;
    loop {
        let event = browser_event(&mut ws).await;
        if event.operation_id.as_deref() == Some("blocked-join") {
            assert!(
                matches!(
                    event.event,
                    GatewayEvent::Outcome {
                        status: OutcomeStatus::Rejected,
                        error: Some(ServiceError {
                            code: ErrorCode::OperationPending,
                            ..
                        }),
                        ..
                    }
                ),
                "canceled uncorrelated FindLot must prevent another admission until its response is drained"
            );
            break;
        }
    }
    release_cancelled.send(()).unwrap();
    loop {
        let event = browser_event(&mut ws).await;
        if let GatewayEvent::Session { session } = event.event {
            assert_eq!(session.state, SessionState::CityReady);
            assert!(
                session.lot_incarnation.is_none(),
                "drained reply must never open a lot connection"
            );
            if session
                .capabilities
                .iter()
                .any(|cap| cap.capability == "lot" && cap.available)
            {
                projection = session;
                break;
            }
        }
    }
    send(
        &mut ws,
        epoch,
        "join",
        GatewayOperation::JoinLot {
            lot_location: 55,
            open_if_closed: false,
        },
    )
    .await;
    loop {
        let event = browser_event(&mut ws).await;
        match event.event {
            GatewayEvent::Session { session } => {
                assert_ne!(
                    session.state,
                    SessionState::LotReady,
                    "malformed VM payload cannot establish lot readiness"
                );
                projection = session;
            }
            GatewayEvent::SourceEvent { family, .. } if family == "vm_decode_error" => break,
            GatewayEvent::Outcome {
                status: OutcomeStatus::Accepted,
                ..
            } => panic!("malformed VM payload cannot complete lot admission"),
            _ => {}
        }
    }
    assert!(matches!(
        outcome(&mut ws, "join", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Accepted,
            ..
        }
    ));
    assert_eq!(projection.state, SessionState::LotReady);
    let incarnation = projection.lot_incarnation.unwrap();
    let roof = vec![32, 42, 0, 0, 0, 0, 0, 0, 63, 17, 0, 0, 0];
    send(
        &mut ws,
        epoch,
        "stale-lot",
        GatewayOperation::LotCommand {
            lot_incarnation: incarnation + 1,
            data: roof.clone(),
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "stale-lot", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Rejected,
            ..
        }
    ));
    let mut forged = roof.clone();
    forged[1] = 43;
    send(
        &mut ws,
        epoch,
        "forged-actor",
        GatewayOperation::LotCommand {
            lot_incarnation: incarnation,
            data: forged,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "forged-actor", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Rejected,
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "roof",
        GatewayOperation::LotCommand {
            lot_incarnation: incarnation,
            data: roof,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "roof", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Unknown,
            ..
        }
    ));
    let eod_incarnation = loop {
        let event = browser_event(&mut ws).await;
        if let GatewayEvent::SourceEvent { family, data, .. } = event.event
            && family == "eod"
        {
            assert_eq!(data["lot_incarnation"], incarnation);
            break data["incarnation"].as_u64().unwrap();
        }
    };
    send(
        &mut ws,
        epoch,
        "wrong-eod",
        GatewayOperation::Eod {
            incarnation: eod_incarnation,
            plugin_id: 17,
            event_name: "change".into(),
            text: Some("77".into()),
            binary: None,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "wrong-eod", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Rejected,
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "dresser",
        GatewayOperation::Eod {
            incarnation: eod_incarnation,
            plugin_id: 0x8b300068,
            event_name: "dresser_change_outfit".into(),
            text: Some("77".into()),
            binary: None,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "dresser", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Unknown,
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "stale-walk",
        GatewayOperation::WalkTo {
            lot_incarnation: incarnation + 1,
            interaction: 25,
            param0: -7,
            x: 160,
            y: 320,
            level: 2,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "stale-walk", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Rejected,
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "walk",
        GatewayOperation::WalkTo {
            lot_incarnation: incarnation,
            interaction: 25,
            param0: -7,
            x: 160,
            y: 320,
            level: 2,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "walk", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Unknown,
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "stale-cancel",
        GatewayOperation::CancelInteraction {
            lot_incarnation: incarnation + 1,
            action_uid: 5,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "stale-cancel", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Rejected,
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "cancel",
        GatewayOperation::CancelInteraction {
            lot_incarnation: incarnation,
            action_uid: 5,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "cancel", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Unknown,
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "refresh",
        GatewayOperation::RequestWorldSnapshot {
            lot_incarnation: incarnation,
        },
    )
    .await;
    assert!(matches!(
        browser_event(&mut ws).await.event,
        GatewayEvent::Pending { .. }
    ));
    snapshot_read.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(40), ws.next())
            .await
            .is_err(),
        "resync write cannot confirm a snapshot"
    );
    release_snapshot.send(()).unwrap();
    assert!(
        matches!(outcome(&mut ws,"refresh",&mut projection).await,GatewayEvent::Outcome{status:OutcomeStatus::Accepted,data,..} if data["snapshot_received"]==true && data["lot_incarnation"]==incarnation)
    );
    assert!(
        !projection
            .capabilities
            .iter()
            .any(|cap| cap.capability == "eod" && cap.available)
    );
    send(&mut ws, epoch, "leave", GatewayOperation::LeaveLot).await;
    assert!(matches!(
        outcome(&mut ws, "leave", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Accepted,
            ..
        }
    ));
    assert!(projection.lot_incarnation.is_none());
    send(
        &mut ws,
        epoch,
        "rejoin",
        GatewayOperation::JoinLot {
            lot_location: 55,
            open_if_closed: false,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "rejoin", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Accepted,
            ..
        }
    ));
    let new_incarnation = projection.lot_incarnation.unwrap();
    assert_ne!(incarnation, new_incarnation);
    send(
        &mut ws,
        epoch,
        "old-incarnation",
        GatewayOperation::CancelInteraction {
            lot_incarnation: incarnation,
            action_uid: 5,
        },
    )
    .await;
    assert!(matches!(
        outcome(&mut ws, "old-incarnation", &mut projection).await,
        GatewayEvent::Outcome {
            status: OutcomeStatus::Rejected,
            ..
        }
    ));
    send(
        &mut ws,
        epoch,
        "unanswered-refresh",
        GatewayOperation::RequestWorldSnapshot {
            lot_incarnation: new_incarnation,
        },
    )
    .await;
    loop {
        if matches!(
            browser_event(&mut ws).await.event,
            GatewayEvent::Pending { .. }
        ) {
            break;
        }
    }
    second_snapshot_read.await.unwrap();
    send(&mut ws, epoch, "leave-again", GatewayOperation::LeaveLot).await;
    let mut refresh_unknown = false;
    let mut leave_events = Vec::new();
    loop {
        let event = browser_event(&mut ws).await;
        leave_events.push(format!("{:?}: {:?}", event.operation_id, event.event));
        match event.event {
            GatewayEvent::Outcome {
                status: OutcomeStatus::Unknown,
                ..
            } if event.operation_id.as_deref() == Some("unanswered-refresh") => {
                refresh_unknown = true
            }
            GatewayEvent::Outcome {
                status: OutcomeStatus::Accepted,
                ..
            } if event.operation_id.as_deref() == Some("leave-again") => break,
            _ => {}
        }
    }
    assert!(
        refresh_unknown,
        "leaving must end a pending snapshot request with an unknown outcome: {leave_events:?}"
    );
    client
        .delete(format!("{url}/v1/session"))
        .bearer_auth(&created.session_token)
        .send()
        .await
        .unwrap();
    city_end.send(()).unwrap();
    lot_end.send(()).unwrap();
    city_task.await.unwrap();
    lot_task.await.unwrap();
    api_task.abort();
    gateway_task.abort();
}
