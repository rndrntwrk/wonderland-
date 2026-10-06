//! Per-native-lot source chat projection and delivery identity.
use wonderland_game_services::{ErrorCode, ServiceError, ServiceResult};
use wonderland_vm_protocol::{
    DecodeLimits,
    chat::{ChatProjection, LotChatDelivery, SourceChatObserver},
};

pub struct LotChatGate {
    observer: SourceChatObserver,
    lot_incarnation: u64,
    sequence: u64,
    previous: Option<ChatProjection>,
}

impl LotChatGate {
    pub fn new(viewer_id: u32, lot_location: u32, lot_incarnation: u64) -> Self {
        Self {
            observer: SourceChatObserver::new(viewer_id, lot_location),
            lot_incarnation,
            sequence: 0,
            previous: None,
        }
    }

    pub fn reset(&mut self, viewer_id: u32, lot_location: u32, lot_incarnation: u64) {
        *self = Self::new(viewer_id, lot_location, lot_incarnation);
    }

    /// The native peer, selected by authenticated admission, owns this input.
    /// Invalid packets clear source chat authority and publish that loss. The
    /// existing VM decoder error event separately reports the protocol failure.
    pub fn observe(&mut self, direct: bool, data: &[u8]) -> ServiceResult<Option<LotChatDelivery>> {
        let limits = DecodeLimits {
            max_input_bytes: wonderland_game_services::protocol::MAX_ARIES_PAYLOAD,
            ..DecodeLimits::default()
        };
        let projection = self
            .observer
            .observe_frame(direct, data, &limits)
            .unwrap_or_else(|_| self.observer.projection());
        let mut metadata = projection.clone();
        metadata.messages.clear();
        if projection.messages.is_empty() && self.previous.as_ref() == Some(&metadata) {
            return Ok(None);
        }
        self.sequence = self.sequence.checked_add(1).ok_or_else(|| {
            ServiceError::new(
                ErrorCode::ResourceBusy,
                "Lot chat delivery identifiers are exhausted; reconnect",
            )
        })?;
        self.previous = Some(metadata);
        Ok(Some(LotChatDelivery {
            lot_incarnation: self.lot_incarnation,
            sequence: self.sequence,
            projection,
        }))
    }
}
