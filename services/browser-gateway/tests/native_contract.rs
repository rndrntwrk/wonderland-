use std::{collections::BTreeMap, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use wonderland_browser_gateway::{config::DestinationAllowlist, native};
use wonderland_game_services::{ErrorCode, SecretString, protocol::*};

async fn read_packet(stream: &mut TcpStream) -> Packet {
    let mut header = [0; 12];
    stream.read_exact(&mut header).await.unwrap();
    let length = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    assert!(length < 4096);
    let mut frame = header.to_vec();
    frame.resize(12 + length, 0);
    stream.read_exact(&mut frame[12..]).await.unwrap();
    decode_frame(&frame, 4096).unwrap().1.remove(0)
}

#[tokio::test]
async fn original_city_handshake_uses_only_its_selected_ticket_and_host_acceptance() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let policy = DestinationAllowlist::from_pinned(BTreeMap::from([(
        "city.example:101".into(),
        vec![listener.local_addr().unwrap()],
    )]))
    .unwrap();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket
            .write_all(&[22, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
            .await
            .unwrap();
        let reply = read_packet(&mut socket).await;
        assert_eq!(reply.packet_type, 21);
        assert_eq!(&reply.body[..3], b"42\0");
        assert_eq!(&reply.body[324..], b"11111111111111111111111111111111");
        socket
            .write_all(&[
                0, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 30, 0, 0, 0, 12, 0, 0, 0, 0, 16, 0,
            ])
            .await
            .unwrap();
        assert_eq!(read_packet(&mut socket).await.packet_type, 10);
        assert_eq!(read_packet(&mut socket).await.packet_type, 0x34);
        assert_eq!(read_packet(&mut socket).await.packet_type, 0x36);
    });
    let connection = tokio::time::timeout(
        Duration::from_secs(5),
        native::connect(
            &policy,
            "city.example:",
            "42",
            &SecretString::new("11111111111111111111111111111111".into()),
            true,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(connection.queued.is_empty());
    peer.await.unwrap();
}

#[tokio::test]
async fn host_online_without_ticket_challenge_is_not_authenticated() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let policy = DestinationAllowlist::from_pinned(BTreeMap::from([(
        "lot.example:101".into(),
        vec![listener.local_addr().unwrap()],
    )]))
    .unwrap();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket
            .write_all(&[
                0, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 30, 0, 0, 0, 12, 0, 0, 0, 0, 16, 0,
            ])
            .await
            .unwrap();
    });
    let result = native::connect(
        &policy,
        "lot.example:",
        "42",
        &SecretString::new("22222222222222222222222222222222".into()),
        false,
    )
    .await;
    assert_eq!(result.err().unwrap().code, ErrorCode::InvalidResponse);
    peer.await.unwrap();
}

#[tokio::test]
async fn stalled_native_write_times_out_after_partial_delivery_without_claiming_acceptance() {
    let (mut writer, mut reader) = tokio::io::duplex(1);
    let error = tokio::time::timeout(
        Duration::from_secs(6),
        native::write_packet(&mut writer, &[1, 2]),
    )
    .await
    .expect("native write must have a finite deadline")
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
    let mut delivered = [0];
    reader.read_exact(&mut delivered).await.unwrap();
    assert_eq!(
        delivered,
        [1],
        "partial write is possible and must never be retried as an accepted command"
    );
}
