//! Original Aries, Voltron and Electron wire codec.
use crate::GatewayOperation;
use crate::{ErrorCode, ServiceError, ServiceResult};
use serde_json::Value;
mod wire;
use wire::{Reader, Writer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub channel: u32,
    pub packet_type: u32,
    pub body: Vec<u8>,
}
pub const MAX_ARIES_PAYLOAD: usize = 4 * 1024 * 1024;
fn invalid() -> ServiceError {
    ServiceError::new(
        ErrorCode::InvalidResponse,
        "Malformed original protocol packet",
    )
}

/// Incomplete data returns (0, []). Invalid declared lengths fail before allocating payloads.
pub fn decode_frame(bytes: &[u8], max_payload: usize) -> ServiceResult<(usize, Vec<Packet>)> {
    if bytes.len() < 12 {
        return Ok((0, Vec::new()));
    }
    let channel = u32::from_le_bytes(bytes[..4].try_into().unwrap());
    let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if size > max_payload {
        return Err(ServiceError::new(
            ErrorCode::ResponseTooLarge,
            "Original frame exceeds the transport byte budget",
        ));
    }
    let total = 12usize.checked_add(size).ok_or_else(invalid)?;
    if bytes.len() < total {
        return Ok((0, Vec::new()));
    }
    if !matches!(channel, 0 | 1000 | 1001) {
        return Ok((
            total,
            vec![Packet {
                channel,
                packet_type: channel,
                body: bytes[12..total].to_vec(),
            }],
        ));
    }
    let mut offset = 12;
    let mut packets = Vec::new();
    while offset < total {
        if total - offset < 6 {
            return Err(invalid());
        }
        let packet_type = u16::from_be_bytes(bytes[offset..offset + 2].try_into().unwrap()) as u32;
        let length = u32::from_be_bytes(bytes[offset + 2..offset + 6].try_into().unwrap()) as usize;
        if length < 6 || length > total - offset {
            return Err(invalid());
        }
        packets.push(Packet {
            channel,
            packet_type,
            body: bytes[offset + 6..offset + length].to_vec(),
        });
        offset += length;
    }
    Ok((total, packets))
}

pub fn encode_packet(channel: u32, packet_type: u32, body: &[u8]) -> ServiceResult<Vec<u8>> {
    let wrapped = matches!(channel, 0 | 1000 | 1001);
    if body.len() > MAX_ARIES_PAYLOAD - 6
        || (wrapped && packet_type > u16::MAX as u32)
        || (!wrapped && channel != packet_type)
    {
        return Err(ServiceError::new(
            ErrorCode::InvalidRequest,
            "Original packet exceeds the transport byte budget",
        ));
    }
    let size = body.len() + if wrapped { 6 } else { 0 };
    let mut out = Vec::with_capacity(12 + size);
    out.extend(channel.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend((size as u32).to_le_bytes());
    if wrapped {
        out.extend((packet_type as u16).to_be_bytes());
        out.extend((size as u32).to_be_bytes());
    }
    out.extend(body);
    Ok(out)
}

pub fn session_response(user: &str, ticket: &str) -> ServiceResult<Vec<u8>> {
    if user.is_empty()
        || user.len() > 112
        || !user.is_ascii()
        || user.contains('\0')
        || ticket.len() != 32
        || !ticket.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(ServiceError::new(
            ErrorCode::InvalidResponse,
            "Invalid server-issued session credentials",
        ));
    }
    let mut body = vec![0; 356];
    body[..user.len()].copy_from_slice(user.as_bytes());
    body[318] = 39;
    body[322..324].copy_from_slice(&4u16.to_le_bytes());
    body[324..].copy_from_slice(ticket.as_bytes());
    encode_packet(21, 21, &body)
}

#[derive(Clone, Debug)]
pub struct EncodedOperation {
    pub bytes: Vec<u8>,
    pub family: String,
    pub has_response: bool,
    pub lot: bool,
}

#[derive(Clone, Debug)]
pub enum SourcePacket {
    SessionChallenge,
    HostOnline,
    ServerBye,
    FindLot {
        status: u16,
        lot_location: u32,
        ticket: crate::SecretString,
        address: String,
        user: String,
    },
    Response {
        family: String,
        source_code: u16,
        accepted: bool,
        data: Value,
    },
    Event {
        family: String,
        source_code: Option<u16>,
        data: Value,
    },
    VmFrame {
        direct: bool,
        data: Vec<u8>,
    },
    Unhandled {
        channel: u32,
        packet_type: u32,
    },
}

pub fn encode_operation(
    operation: &GatewayOperation,
    actor: u32,
    actor_name: &str,
    operation_id: &str,
) -> ServiceResult<EncodedOperation> {
    use crate::{BulletinAction, Gender, NeighborhoodAction, RoommateAction, SkinTone};
    use GatewayOperation::*;
    let mut w = Writer::default();
    let (channel, packet_type, family, has_response, lot) = match operation {
        JoinLot {
            lot_location,
            open_if_closed,
        } => {
            w.u32(*lot_location);
            w.byte(u8::from(*open_if_closed));
            (1000, 5, "find_lot".into(), true, false)
        }
        FindAvatar { avatar_id } => {
            w.u32(*avatar_id);
            (1000, 10, format!("find_avatar:{avatar_id}"), true, false)
        }
        CreateAvatar {
            name,
            description,
            gender,
            skin,
            head_key,
            body_key,
        } => {
            w.bytes(&[0; 37]);
            w.vlc(name)?;
            w.vlc(description)?;
            w.byte(u8::from(*gender == Gender::Female));
            w.byte(match skin {
                SkinTone::Light => 0,
                SkinTone::Medium => 1,
                SkinTone::Dark => 2,
            });
            w.u32((head_key.0 >> 32) as u32);
            w.u32(0);
            w.u32((body_key.0 >> 32) as u32);
            (0, 0x2730, "create_avatar".into(), true, false)
        }
        RetireAvatar => (1000, 17, "retire_avatar".into(), false, false),
        PrivateMessage {
            target_avatar_id,
            message,
            color,
        } => {
            w.u16(5);
            w.u32(actor);
            w.u32(*target_avatar_id);
            w.u16(0);
            w.vlc(message)?;
            w.vlc(operation_id)?;
            w.u16(0);
            w.u32(*color);
            (
                1000,
                4,
                format!("instant_message:{operation_id}"),
                true,
                false,
            )
        }
        MailPoll { since_ticks } => {
            if since_ticks.0 > 3_155_378_975_999_999_999 {
                return Err(ServiceError::new(
                    ErrorCode::InvalidRequest,
                    "Invalid original DateTime ticks",
                ));
            }
            w.u16(0);
            w.u64(since_ticks.0);
            (1000, 18, "mail_poll".into(), true, false)
        }
        MailDelete { message_id } => {
            if *message_id <= 0 {
                return Err(ServiceError::new(
                    ErrorCode::InvalidRequest,
                    "Invalid mail identifier",
                ));
            }
            w.u16(2);
            w.u64(*message_id as u64);
            (1000, 18, "mail_delete".into(), false, false)
        }
        MailSend {
            target_avatar_id,
            subject,
            body,
        } => {
            let mut m = Writer::little_endian();
            m.bytes(b"FSOI");
            m.u32(1);
            m.u32(0);
            m.u32(actor);
            m.u32(*target_avatar_id);
            m.long_ascii(subject)?;
            m.long_ascii(body)?;
            m.long_ascii(actor_name)?;
            m.u64(0);
            m.u32(0);
            m.u32(0);
            m.u32(0);
            m.byte(0);
            w.u16(1);
            w.u32(m.0.len() as u32);
            w.bytes(&m.0);
            (1000, 18, "mail_send".into(), true, false)
        }
        PurchaseLot {
            x,
            y,
            name,
            start_fresh,
            mayor_mode,
        } => {
            w.u16(*x);
            w.u16(*y);
            w.pascal(name)?;
            w.byte(u8::from(*start_fresh));
            w.byte(u8::from(*mayor_mode));
            (1000, 2, "purchase_lot".into(), true, false)
        }
        Roommate {
            action,
            avatar_id,
            lot_location,
        } => {
            w.u16(match action {
                RoommateAction::Invite => 0,
                RoommateAction::Kick => 1,
                RoommateAction::Accept => 2,
                RoommateAction::Decline => 3,
                RoommateAction::Poll => 4,
            });
            w.u32(*avatar_id);
            w.u32(*lot_location);
            (
                1000,
                12,
                "roommate".into(),
                *action != RoommateAction::Poll,
                false,
            )
        }
        Neighborhood {
            action,
            target_avatar_id,
            neighborhood_id,
            message,
            value,
        } => {
            w.u16(match action {
                NeighborhoodAction::Vote => 0,
                NeighborhoodAction::CanVote => 1,
                NeighborhoodAction::Nominate => 2,
                NeighborhoodAction::CanNominate => 3,
                NeighborhoodAction::Rate => 4,
                NeighborhoodAction::CanRate => 5,
                NeighborhoodAction::NominationRun => 6,
                NeighborhoodAction::CanRun => 7,
                NeighborhoodAction::CanFreeVote => 8,
                NeighborhoodAction::FreeVote => 9,
            });
            w.u32(*target_avatar_id);
            w.u32(*neighborhood_id);
            w.pascal(message)?;
            w.u32(*value);
            (1000, 20, "neighborhood".into(), true, false)
        }
        Bulletin {
            action,
            neighborhood_id,
            title,
            message,
            lot_id,
            value,
        } => {
            let kind = match action {
                BulletinAction::GetMessages => 0,
                BulletinAction::PostMessage => 1,
                BulletinAction::PromoteMessage => 2,
                BulletinAction::CanPostMessage => 3,
                BulletinAction::DeleteMessage => 6,
            };
            w.u16(kind);
            w.u32(*neighborhood_id);
            if kind == 1 {
                w.pascal(title)?;
                w.pascal(message)?;
                w.u32(*lot_id);
            }
            w.u32(*value);
            (1000, 23, "bulletin".into(), true, false)
        }
        LotChat { message } => {
            if message.is_empty() || message.encode_utf16().count() > 200 {
                return Err(ServiceError::new(
                    ErrorCode::InvalidRequest,
                    "Original lot chat accepts 1–200 characters",
                ));
            }
            let mut command = Writer::little_endian();
            command.byte(4);
            command.u32(actor);
            command.vlc(message)?;
            command.byte(0);
            w.u32(command.0.len() as u32);
            w.bytes(&command.0);
            (1000, 9, "lot_chat".into(), false, true)
        }
        CancelInteraction { action_uid, .. } => {
            let mut command = Writer::little_endian();
            command.byte(7);
            command.u32(actor);
            command.u16(*action_uid);
            w.u32(command.0.len() as u32);
            w.bytes(&command.0);
            (1000, 9, "cancel_interaction".into(), false, true)
        }
        WalkTo {
            interaction,
            param0,
            x,
            y,
            level,
            ..
        } => {
            let mut command = Writer::little_endian();
            command.byte(10);
            command.u32(actor);
            command.u16(*interaction);
            command.u16(*param0 as u16);
            command.u16(*x as u16);
            command.u16(*y as u16);
            command.byte(*level as u8);
            w.u32(command.0.len() as u32);
            w.bytes(&command.0);
            (1000, 9, "walk".into(), false, true)
        }
        Eod {
            plugin_id,
            event_name,
            text,
            binary,
            ..
        } => {
            if event_name.is_empty()
                || event_name.len() > 128
                || matches!(event_name.as_str(), "eod_enter" | "eod_leave")
            {
                return Err(ServiceError::new(
                    ErrorCode::InvalidRequest,
                    "Invalid outbound EOD event",
                ));
            }
            let mut command = Writer::little_endian();
            command.byte(18);
            command.u32(actor);
            command.u32(*plugin_id);
            command.vlc(event_name)?;
            match (text, binary) {
                (Some(text), None) => {
                    command.byte(0);
                    command.vlc(text)?;
                }
                (None, Some(binary)) if binary.len() <= usize::from(u16::MAX) => {
                    command.byte(1);
                    command.u16(binary.len() as u16);
                    command.bytes(binary);
                }
                _ => {
                    return Err(ServiceError::new(
                        ErrorCode::InvalidRequest,
                        "EOD events require exactly one payload within the original 16-bit binary length",
                    ));
                }
            }
            w.u32(command.0.len() as u32);
            w.bytes(&command.0);
            (1000, 9, "eod".into(), false, true)
        }
        _ => {
            return Err(ServiceError::new(
                ErrorCode::Unsupported,
                "This operation is handled by the gateway session lifecycle",
            ));
        }
    };
    Ok(EncodedOperation {
        bytes: encode_packet(channel, packet_type, &w.0)?,
        family,
        has_response,
        lot,
    })
}

/// Receipt ACK for a message delivered to the browser; does not imply the recipient read it.
pub fn instant_message_ack(actor: u32, target: u32, ack_id: &str) -> ServiceResult<Vec<u8>> {
    let mut w = Writer::default();
    w.u16(5);
    w.u32(actor);
    w.u32(target);
    w.u16(1);
    w.vlc("")?;
    w.vlc(ack_id)?;
    w.u16(0);
    w.u32(0);
    encode_packet(1000, 4, &w.0)
}

pub fn parse_packet(packet: &Packet) -> ServiceResult<SourcePacket> {
    use serde_json::json;
    let mut r = Reader::new(&packet.body);
    let parsed = match (packet.channel, packet.packet_type) {
        (22, 22) => SourcePacket::SessionChallenge,
        (0, 0x1e) => {
            r.take(6)?;
            SourcePacket::HostOnline
        }
        (0, 7) => {
            r.u32()?;
            r.pascal()?;
            r.pascal()?;
            SourcePacket::ServerBye
        }
        (1000, 6) => {
            let status = r.u16()?;
            if status > 7 {
                return Err(invalid());
            }
            let lot_location = r.u32()?;
            let ticket = r.vlc()?;
            let address = r.vlc()?;
            let user = r.vlc()?;
            if status == 0
                && (ticket.len() != 32
                    || !ticket.bytes().all(|b| b.is_ascii_hexdigit())
                    || address.len() > 256
                    || address.is_empty()
                    || user.is_empty()
                    || user.len() > 112
                    || !user.is_ascii())
            {
                return Err(invalid());
            }
            SourcePacket::FindLot {
                status,
                lot_location,
                ticket: crate::SecretString::new(ticket),
                address,
                user,
            }
        }
        (1000, 1) => {
            let status = r.u16()?;
            let reason = r.u16()?;
            let id = r.u32()?;
            if !matches!(status, 1 | 2) {
                return Err(invalid());
            }
            response(
                "create_avatar",
                reason,
                status == 1,
                json!({"status":status,"reason":reason,"avatar_id":id}),
            )
        }
        (1000, 3) => {
            let status = r.u16()?;
            let reason = r.u16()?;
            let lot_id = r.u32()?;
            let funds = r.u32()? as i32;
            if !matches!(status, 1 | 2) {
                return Err(invalid());
            }
            response(
                "purchase_lot",
                reason,
                status == 1,
                json!({"status":status,"reason":reason,"new_lot_id":lot_id,"new_funds":funds}),
            )
        }
        (1000, 4) => {
            let from_type = r.u16()?;
            let from = r.u32()?;
            let to = r.u32()?;
            let kind = r.u16()?;
            let message = r.vlc()?;
            let ack_id = r.vlc()?;
            let reason = r.u16()?;
            let color = r.u32()?;
            let data = json!({"from_type":from_type,"from":from,"to":to,"type":kind,"message":message,"ack_id":ack_id,"reason":reason,"color":color});
            match kind {
                0 => SourcePacket::Event {
                    family: "instant_message".into(),
                    source_code: None,
                    data,
                },
                1 | 2 => response(
                    &format!("instant_message:{ack_id}"),
                    reason,
                    kind == 1,
                    data,
                ),
                _ => return Err(invalid()),
            }
        }
        (1000, 11) => {
            let id = r.u32()?;
            let status = r.u16()?;
            let location = r.u32()?;
            if status > 4 {
                return Err(invalid());
            }
            response(
                &format!("find_avatar:{id}"),
                status,
                status == 0,
                json!({"avatar_id":id,"status":status,"lot_location":location}),
            )
        }
        (1000, 12) => {
            let action = r.u16()?;
            let avatar_id = r.u32()?;
            let lot_location = r.u32()?;
            if action != 0 {
                return Err(invalid());
            }
            SourcePacket::Event {
                family: "roommate_invitation".into(),
                source_code: Some(action),
                data: json!({"action":"invite","avatar_id":avatar_id,"lot_location":lot_location}),
            }
        }
        (1000, 14) => {
            let code = r.u16()?;
            let extra = r.u32()?;
            let data = json!({"code":code,"extra":extra});
            if matches!(code, 13 | 14) {
                SourcePacket::Event {
                    family: "roommate".into(),
                    source_code: Some(code),
                    data,
                }
            } else {
                response("roommate", code, matches!(code, 0 | 9 | 10 | 11 | 12), data)
            }
        }
        (1000, 19) => {
            let kind = r.u16()?;
            if kind > 7 {
                return Err(invalid());
            }
            let messages = read_items(&mut r, false)?;
            let data = json!({"type":kind,"messages":messages});
            match kind {
                0 => response("mail_poll", kind, true, data),
                1 => SourcePacket::Event {
                    family: "mail".into(),
                    source_code: Some(kind),
                    data,
                },
                _ => response("mail_send", kind, kind == 7, data),
            }
        }
        (1000, 21) => {
            let code = r.u16()?;
            let ban_end_date = r.u32()?;
            let message = r.vlc()?;
            response(
                "neighborhood",
                code,
                code == 0,
                json!({"code":code,"ban_end_date":ban_end_date,"message":message}),
            )
        }
        (1000, 22) => {
            let nomination_mode = r.bool()?;
            let count = r.count(10)?;
            let mut candidates = Vec::with_capacity(count);
            for _ in 0..count {
                let id = r.u32()?;
                let name = r.vlc()?;
                let rating = r.u32()?;
                let mut data = json!({"avatar_id":id,"name":name,"rating":rating});
                if !nomination_mode {
                    data["last_neighborhood_name"] = json!(r.vlc()?);
                    data["last_neighborhood_id"] = json!(r.u32()?);
                    data["term_number"] = json!(r.u32()?);
                    data["message"] = json!(r.vlc()?);
                }
                candidates.push(data);
            }
            SourcePacket::Event {
                family: "neighborhood_candidates".into(),
                source_code: None,
                data: json!({"nomination_mode":nomination_mode,"candidates":candidates}),
            }
        }
        (1000, 24) => {
            let code = r.u16()?;
            let messages = read_items(&mut r, true)?;
            let message = r.vlc()?;
            let ban_end_date = r.u32()?;
            response(
                "bulletin",
                code,
                code <= 2,
                json!({"code":code,"messages":messages,"message":message,"ban_end_date":ban_end_date}),
            )
        }
        (1000, 7 | 8) => {
            let size = r.u32()? as usize;
            let data = r.take(size)?.to_vec();
            SourcePacket::VmFrame {
                direct: packet.packet_type == 8,
                data,
            }
        }
        (1000, 16) => {
            let use_cst = r.bool()?;
            let title = r.vlc()?;
            let message = r.vlc()?;
            SourcePacket::Event {
                family: "protocol_message".into(),
                source_code: None,
                data: json!({"use_cst":use_cst,"title":title,"message":message}),
            }
        }
        _ => {
            return Ok(SourcePacket::Unhandled {
                channel: packet.channel,
                packet_type: packet.packet_type,
            });
        }
    };
    r.finish()?;
    Ok(parsed)
}

fn response(family: &str, source_code: u16, accepted: bool, data: Value) -> SourcePacket {
    SourcePacket::Response {
        family: family.into(),
        source_code,
        accepted,
        data,
    }
}

fn read_items(r: &mut Reader<'_>, bulletin: bool) -> ServiceResult<Vec<Value>> {
    use serde_json::json;
    let count = r.count(50)?;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let size = r.u32()? as usize;
        let mut item = Reader::little_endian(r.take(size)?);
        if item.take(4)? != if bulletin { b"FSOB" } else { b"FSOI" } || item.u32()? != 1 {
            return Err(invalid());
        }
        let id = item.u32()?;
        let (sender, target, nhood) = if bulletin {
            let nhood = item.u32()?;
            (item.u32()?, 0, nhood)
        } else {
            (item.u32()?, item.u32()?, 0)
        };
        let subject = item.long_ascii()?;
        let body = item.long_ascii()?;
        let sender_name = item.long_ascii()?;
        let time_ticks = item.u64()?.to_string();
        let kind = item.u32()?;
        let mut data = json!({"id":id,"sender_id":sender,"subject":subject,"body":body,"sender_name":sender_name,"time_ticks":time_ticks,"type":kind});
        if bulletin {
            data["neighborhood_id"] = json!(nhood);
            data["flags"] = json!(item.u32()?);
            data["lot_id"] = json!(item.u32()?);
        } else {
            data["target_id"] = json!(target);
            data["subtype"] = json!(item.u32()?);
            data["read_state"] = json!(item.u32()?);
            data["reply_id"] = if item.bool()? {
                json!(item.u32()?)
            } else {
                Value::Null
            };
        }
        item.finish()?;
        out.push(data);
    }
    Ok(out)
}

/// Wrap a previously validated complete original VMNetCommand. Admission belongs to the gateway.
pub fn wrap_vm_command(data: &[u8]) -> ServiceResult<Vec<u8>> {
    if data.len() > 128 * 1024 || data.is_empty() {
        return Err(ServiceError::new(
            ErrorCode::InvalidRequest,
            "VM command exceeds the transport byte budget",
        ));
    }
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend((data.len() as u32).to_be_bytes());
    body.extend(data);
    encode_packet(1000, 9, &body)
}

pub fn request_world_snapshot(tick_id: u32) -> ServiceResult<Vec<u8>> {
    // VMRequestResyncCmd deliberately does not serialize VMNetCommandBodyAbstract/ActorUID.
    let mut command = vec![13];
    command.extend(0u32.to_le_bytes());
    command.extend(tick_id.to_le_bytes());
    wrap_vm_command(&command)
}
