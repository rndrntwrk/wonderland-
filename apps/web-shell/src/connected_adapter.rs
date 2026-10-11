//! Browser-owned request correlation. This state contains no credentials or tokens.
pub mod lot_chat;
pub mod state;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use wonderland_game_services::{DirectoryQuery, DirectoryResult, ErrorCode, ServiceError};

/// Matches the source HTTP adapter's whole-response quota; it is not a row limit.
pub const MAX_RESPONSE_BYTES: usize = wonderland_game_services::MAX_HTTP_BODY;
pub const MAX_VM_FRAME_BYTES: usize = wonderland_game_services::protocol::MAX_ARIES_PAYLOAD;
/// A VM envelope has only fixed field names, booleans, two u64s, and a <=96-byte operation ID.
pub const MAX_GATEWAY_METADATA_BYTES: usize = 4096;
/// serde JSON encodes each u8 as at most three decimal digits followed by a comma.
pub const MAX_GATEWAY_ENVELOPE_BYTES: usize = 4 * MAX_VM_FRAME_BYTES + MAX_GATEWAY_METADATA_BYTES;
/// The gateway's incoming WebSocket message/frame quota is 128 KiB.
pub const MAX_GATEWAY_REQUEST_BYTES: usize = 128 * 1024;

pub fn decode_gateway_envelope(
    body: &[u8],
) -> Result<wonderland_game_services::GatewayEnvelope, ServiceError> {
    let envelope: wonderland_game_services::GatewayEnvelope =
        decode_with_budget(body, MAX_GATEWAY_ENVELOPE_BYTES)?;
    if matches!(&envelope.event, wonderland_game_services::GatewayEvent::VmFrame {data,..} if data.len()>MAX_VM_FRAME_BYTES)
    {
        return Err(ServiceError::new(
            ErrorCode::ResponseTooLarge,
            "The world frame exceeds the original protocol budget.",
        ));
    }
    Ok(envelope)
}

pub fn decode_bounded<T: DeserializeOwned>(body: &[u8]) -> Result<T, ServiceError> {
    decode_with_budget(body, MAX_RESPONSE_BYTES)
}

fn decode_with_budget<T: DeserializeOwned>(body: &[u8], maximum: usize) -> Result<T, ServiceError> {
    if body.len() > maximum {
        return Err(ServiceError::new(
            ErrorCode::ResponseTooLarge,
            "The service response is too large. Narrow this search and try again.",
        ));
    }
    serde_json::from_slice(body).map_err(|_| {
        ServiceError::new(
            ErrorCode::InvalidResponse,
            "The service returned an invalid response.",
        )
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DirectoryPage {
    pub rows: Vec<Value>,
    pub page: u32,
    pub total_pages: Option<u32>,
    pub total_items: Option<u64>,
}

impl DirectoryPage {
    pub fn from_result(result: &DirectoryResult) -> Self {
        let data = &result.data;
        let rows = [
            "avatars",
            "lots",
            "neighborhoods",
            "bulletins",
            "candidates",
        ]
        .iter()
        .find_map(|key| data.get(key).and_then(Value::as_array))
        .or_else(|| data.as_array())
        .cloned()
        .unwrap_or_else(|| {
            if data.is_object() {
                vec![data.clone()]
            } else {
                vec![]
            }
        });
        let requested_page = match result.query {
            DirectoryQuery::AvatarPage { page, .. } | DirectoryQuery::LotPage { page, .. } => page,
            _ => 1,
        };
        Self {
            rows,
            page: source_u32(data, "page").unwrap_or(requested_page),
            total_pages: source_u32(data, "total_pages"),
            total_items: [
                "total_avatars",
                "total_lots",
                "avatars_online_count",
                "total_lots_online",
            ]
            .iter()
            .find_map(|key| data.get(key).and_then(Value::as_u64)),
        }
    }

    pub fn next_query(query: &DirectoryQuery, page: u32) -> Option<DirectoryQuery> {
        if page == 0 {
            return None;
        }
        match query {
            DirectoryQuery::AvatarPage {
                shard_id, per_page, ..
            } => Some(DirectoryQuery::AvatarPage {
                shard_id: *shard_id,
                page,
                per_page: *per_page,
            }),
            DirectoryQuery::LotPage {
                shard_id, per_page, ..
            } => Some(DirectoryQuery::LotPage {
                shard_id: *shard_id,
                page,
                per_page: *per_page,
            }),
            _ => None,
        }
    }
}

pub fn source_u32(data: &Value, key: &str) -> Option<u32> {
    data.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| n.try_into().ok())
}

pub fn source_text(data: &Value, key: &str) -> String {
    match data.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => String::new(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestStamp {
    pub epoch: u64,
    pub operation_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationStatus {
    Waiting,
    Pending(String),
    Accepted(String),
    Rejected(String),
    Unknown(String),
}

#[derive(Clone, Debug)]
pub struct PendingOperation {
    pub stamp: RequestStamp,
    pub label: String,
    pub status: OperationStatus,
    pub draft_key: Option<String>,
    pub submitted_draft: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct BrowserLedger {
    pub epoch: u64,
    pub authenticated: bool,
    pub transport_ready: bool,
    pub drafts: BTreeMap<String, String>,
    pub operations: BTreeMap<String, PendingOperation>,
    pub unread: BTreeMap<u32, usize>,
    pub selected_person: Option<u32>,
    pub selected_lot: Option<u32>,
    next_operation: u64,
    reads: BTreeMap<String, RequestStamp>,
    received_messages: BTreeSet<String>,
    login_pending: bool,
}

impl BrowserLedger {
    pub fn begin_login(&mut self) -> u64 {
        self.logout();
        self.login_pending = true;
        self.epoch
    }

    pub fn accept_login(&mut self, epoch: u64) -> bool {
        if epoch != self.epoch || !self.login_pending {
            return false;
        }
        self.login_pending = false;
        self.authenticated = true;
        true
    }

    pub fn logout(&mut self) {
        let epoch = self.epoch.wrapping_add(1);
        *self = Self {
            epoch,
            ..Self::default()
        };
    }

    pub fn open_transport(&mut self, epoch: u64) -> bool {
        if epoch != self.epoch || !self.authenticated {
            return false;
        }
        self.transport_ready = true;
        true
    }

    pub fn close_transport(&mut self, epoch: u64, reason: &str) -> bool {
        if epoch != self.epoch || !self.authenticated {
            return false;
        }
        self.transport_ready = false;
        for operation in self.operations.values_mut() {
            if matches!(
                operation.status,
                OperationStatus::Waiting | OperationStatus::Pending(_)
            ) {
                operation.status = OperationStatus::Unknown(reason.to_owned());
            }
        }
        true
    }

    fn next_stamp(&mut self) -> RequestStamp {
        self.next_operation = self.next_operation.wrapping_add(1);
        RequestStamp {
            epoch: self.epoch,
            operation_id: format!("ui-{}", self.next_operation),
        }
    }

    pub fn begin_operation(
        &mut self,
        label: &str,
        draft_key: Option<&str>,
    ) -> Result<RequestStamp, String> {
        if !self.authenticated || !self.transport_ready {
            return Err("Reconnect before sending this action.".into());
        }
        let stamp = self.next_stamp();
        let operation = PendingOperation {
            stamp: stamp.clone(),
            label: label.to_owned(),
            status: OperationStatus::Waiting,
            draft_key: draft_key.map(str::to_owned),
            submitted_draft: draft_key.and_then(|key| self.drafts.get(key).cloned()),
        };
        self.operations
            .insert(stamp.operation_id.clone(), operation);
        Ok(stamp)
    }

    pub fn receive_operation(&mut self, stamp: &RequestStamp, status: OperationStatus) -> bool {
        if stamp.epoch != self.epoch || !self.authenticated || !self.transport_ready {
            return false;
        }
        let Some(operation) = self.operations.get_mut(&stamp.operation_id) else {
            return false;
        };
        if &operation.stamp != stamp
            || !matches!(
                operation.status,
                OperationStatus::Waiting | OperationStatus::Pending(_)
            )
        {
            return false;
        }
        if matches!(status, OperationStatus::Accepted(_))
            && let (Some(key), Some(submitted)) = (&operation.draft_key, &operation.submitted_draft)
            && self.drafts.get(key) == Some(submitted)
        {
            self.drafts.insert(key.clone(), String::new());
        }
        operation.status = status;
        true
    }

    pub fn begin_read(&mut self, slot: &str) -> RequestStamp {
        let stamp = self.next_stamp();
        self.reads.insert(slot.to_owned(), stamp.clone());
        stamp
    }

    pub fn finish_read(&mut self, slot: &str, stamp: &RequestStamp) -> bool {
        if stamp.epoch != self.epoch || self.reads.get(slot) != Some(stamp) {
            return false;
        }
        self.reads.remove(slot);
        true
    }

    pub fn receive_message(&mut self, epoch: u64, id: &str, person: u32, visible: bool) -> bool {
        if epoch != self.epoch
            || !self.authenticated
            || !self.received_messages.insert(id.to_owned())
        {
            return false;
        }
        if !visible {
            *self.unread.entry(person).or_default() += 1;
        }
        true
    }

    pub fn select_conversation(&mut self, person: u32) {
        self.selected_person = Some(person);
        self.unread.insert(person, 0);
    }
}
