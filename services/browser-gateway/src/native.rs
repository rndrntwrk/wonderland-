use crate::config::DestinationAllowlist;
use std::collections::VecDeque;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};
use wonderland_game_services::{ErrorCode, SecretString, ServiceError, ServiceResult, protocol::*};

pub struct NativeConnection {
    pub stream: TcpStream,
    pub queued: VecDeque<Packet>,
}
pub async fn connect(
    policy: &DestinationAllowlist,
    address: &str,
    user: &str,
    ticket: &SecretString,
    city: bool,
) -> ServiceResult<NativeConnection> {
    let addresses = policy.selected(address)?;
    let response = session_response(user, ticket.expose())?;
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let mut connection = None;
        for address in addresses {
            if let Ok(Ok(stream)) = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                TcpStream::connect(address),
            )
            .await
            {
                connection = Some(stream);
                break;
            }
        }
        let mut stream = connection.ok_or_else(disconnected)?;
        stream.set_nodelay(true).map_err(|_| disconnected())?;
        let mut challenged = false;
        for _ in 0..128 {
            let packets = read_packets(&mut stream).await?;
            let mut iter = packets.into_iter();
            while let Some(packet) = iter.next() {
                match parse_packet(&packet)? {
                    SourcePacket::SessionChallenge if !challenged => {
                        stream
                            .write_all(&response)
                            .await
                            .map_err(|_| disconnected())?;
                        challenged = true;
                    }
                    SourcePacket::HostOnline if challenged => {
                        stream
                            .write_all(&encode_packet(0, 10, &[0; 22])?)
                            .await
                            .map_err(|_| disconnected())?;
                        if city {
                            stream
                                .write_all(&encode_packet(0, 0x34, &[0; 2])?)
                                .await
                                .map_err(|_| disconnected())?;
                            stream
                                .write_all(&encode_packet(0, 0x36, &[0; 4])?)
                                .await
                                .map_err(|_| disconnected())?;
                        }
                        return Ok(NativeConnection {
                            stream,
                            queued: iter.collect(),
                        });
                    }
                    SourcePacket::ServerBye => {
                        return Err(ServiceError::new(
                            ErrorCode::Rejected,
                            "The original server rejected session admission",
                        ));
                    }
                    _ => {
                        return Err(ServiceError::new(
                            ErrorCode::InvalidResponse,
                            "Unexpected packet during original session admission",
                        ));
                    }
                }
            }
        }
        Err(ServiceError::new(
            ErrorCode::InvalidResponse,
            "Original session admission exceeded its handshake packet budget",
        ))
    })
    .await
    .map_err(|_| {
        ServiceError::new(
            ErrorCode::Timeout,
            "Original city or lot session admission timed out",
        )
    })?
}

pub async fn read_packets<R: AsyncRead + Unpin>(reader: &mut R) -> ServiceResult<Vec<Packet>> {
    let mut header = [0u8; 12];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|_| disconnected())?;
    let size = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if size > MAX_ARIES_PAYLOAD {
        return Err(ServiceError::new(
            ErrorCode::ResponseTooLarge,
            "Original frame exceeds the transport byte budget",
        ));
    }
    let mut frame = Vec::with_capacity(12 + size);
    frame.extend(header);
    frame.resize(12 + size, 0);
    reader
        .read_exact(&mut frame[12..])
        .await
        .map_err(|_| disconnected())?;
    Ok(decode_frame(&frame, MAX_ARIES_PAYLOAD)?.1)
}
pub fn disconnected() -> ServiceError {
    ServiceError::new(
        ErrorCode::Disconnected,
        "The original city or lot connection was lost",
    )
}

/// A stalled native write must not hold the session actor beyond its own deadline.
/// Failure may follow a partial write, so callers close the transport without replay.
pub async fn write_packet<W: AsyncWrite + Unpin>(
    writer: &mut W,
    bytes: &[u8],
) -> ServiceResult<()> {
    tokio::time::timeout(std::time::Duration::from_secs(5), writer.write_all(bytes))
        .await
        .map_err(|_| {
            ServiceError::new(
                ErrorCode::Timeout,
                "The original server stopped accepting session data",
            )
        })?
        .map_err(|_| disconnected())
}
