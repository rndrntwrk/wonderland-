//! Browser-owned lot chat history. Authority comes from the native source observer.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use wonderland_game_services::{SessionProjection, SessionState};
use wonderland_vm_protocol::chat::{
    LotChatDelivery, SourceChatKind, SourceChatMessage, VisibleChatChannel,
};

// UIChatDialog.ReceiveEvent retains 100 source history events.
const SOURCE_HISTORY_EVENTS: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    source_epoch: u64,
    lot_location: u32,
    lot_incarnation: u64,
    viewer_id: u32,
    viewer_incarnation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivedLotChatMessage {
    pub id: String,
    pub message: SourceChatMessage,
}

#[derive(Clone, Debug)]
pub struct LotChatLedger {
    identity: Option<Identity>,
    last_sequence: u64,
    history: VecDeque<ReceivedLotChatMessage>,
    unread: BTreeSet<String>,
    shown: BTreeMap<u8, bool>,
    pub channels: Vec<VisibleChatChannel>,
    pub ready: bool,
    pub at_bottom: bool,
    pub scroll_top: i32,
    pub display_revision: u64,
}

impl Default for LotChatLedger {
    fn default() -> Self {
        Self {
            identity: None,
            last_sequence: 0,
            history: VecDeque::new(),
            unread: BTreeSet::new(),
            shown: BTreeMap::new(),
            channels: vec![],
            ready: false,
            at_bottom: true,
            scroll_top: 0,
            display_revision: 0,
        }
    }
}

impl LotChatLedger {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn suspend(&mut self) {
        self.ready = false;
        self.channels.clear();
        self.display_revision = self.display_revision.wrapping_add(1);
    }

    pub fn receive(
        &mut self,
        session: &SessionProjection,
        delivery: LotChatDelivery,
        visible: bool,
    ) -> bool {
        let projection = delivery.projection;
        if session.state != SessionState::LotReady
            || session.lot_incarnation != Some(delivery.lot_incarnation)
            || session.lot_location != Some(projection.lot_location)
            || session.avatar_id != Some(projection.viewer_id)
            || projection.viewer_id == 0
            || delivery.lot_incarnation == 0
            || delivery.sequence == 0
            || (projection.ready && projection.viewer_incarnation == 0)
            || (!projection.ready
                && (!projection.channels.is_empty() || !projection.messages.is_empty()))
        {
            return false;
        }
        let identity = Identity {
            source_epoch: session.epoch,
            lot_location: projection.lot_location,
            lot_incarnation: delivery.lot_incarnation,
            viewer_id: projection.viewer_id,
            viewer_incarnation: projection.viewer_incarnation,
        };
        if let Some(current) = &self.identity
            && (current.source_epoch != identity.source_epoch
                || current.lot_location != identity.lot_location
                || current.lot_incarnation != identity.lot_incarnation
                || current.viewer_id != identity.viewer_id
                || identity.viewer_incarnation < current.viewer_incarnation
                || delivery.sequence <= self.last_sequence)
        {
            return false;
        }
        let mut channel_ids = BTreeSet::new();
        if projection.channels.iter().any(|channel| {
            !matches!(channel.id, 0..=4 | 7)
                || !channel_ids.insert(channel.id)
                || (channel.id == 0 && channel.private)
        }) {
            return false;
        }
        if projection.messages.iter().any(|message| {
            message.sender_uid == 0
                || message.sender_incarnation == 0
                || message.sender_name.is_empty()
                || match message.kind {
                    SourceChatKind::Message => {
                        message.text.is_empty()
                            || message.text.encode_utf16().count() > 200
                            || message.channel_id.is_none()
                    }
                    SourceChatKind::Join => {
                        message.private || message.channel_id.is_some() || !message.text.is_empty()
                    }
                }
        }) {
            return false;
        }
        if self
            .identity
            .as_ref()
            .is_some_and(|current| current.viewer_incarnation != identity.viewer_incarnation)
        {
            self.reset();
        }
        self.identity = Some(identity);
        self.last_sequence = delivery.sequence;
        self.ready = projection.ready;
        self.channels = projection.channels;
        for channel in &self.channels {
            self.shown
                .entry(channel.id)
                .or_insert(channel.show_by_default);
        }
        for (index, message) in projection.messages.into_iter().enumerate() {
            let id = format!(
                "lot:{}:{}:{}",
                projection.viewer_incarnation, delivery.sequence, index
            );
            if message.kind == SourceChatKind::Message
                && message.sender_uid != projection.viewer_id
                && self.message_visible(&message)
                && !(visible && self.at_bottom)
            {
                self.unread.insert(id.clone());
            }
            self.history
                .push_back(ReceivedLotChatMessage { id, message });
            if self.history.len() > SOURCE_HISTORY_EVENTS
                && let Some(removed) = self.history.pop_front()
            {
                self.unread.remove(&removed.id);
            }
        }
        self.display_revision = self.display_revision.wrapping_add(1);
        true
    }

    fn message_visible(&self, message: &SourceChatMessage) -> bool {
        self.ready
            && match message.kind {
                SourceChatKind::Join => !message.private && message.channel_id.is_none(),
                SourceChatKind::Message => message.channel_id.is_some_and(|id| {
                    self.channel_shown(id)
                        && self
                            .channels
                            .iter()
                            .any(|channel| channel.id == id && channel.private == message.private)
                }),
            }
    }

    pub fn visible_messages(&self) -> Vec<ReceivedLotChatMessage> {
        self.history
            .iter()
            .filter(|event| self.message_visible(&event.message))
            .cloned()
            .collect()
    }

    pub fn unread_count(&self) -> usize {
        self.history
            .iter()
            .filter(|event| self.unread.contains(&event.id) && self.message_visible(&event.message))
            .count()
    }

    pub fn mark_read(&mut self) {
        let visible: Vec<_> = self
            .history
            .iter()
            .filter(|event| self.message_visible(&event.message))
            .map(|event| event.id.clone())
            .collect();
        for id in visible {
            self.unread.remove(&id);
        }
    }

    pub fn channel_shown(&self, id: u8) -> bool {
        self.shown.get(&id).copied().unwrap_or(false)
    }

    pub fn toggle_channel(&mut self, id: u8) {
        if self.channels.iter().any(|channel| channel.id == id) {
            self.shown.insert(id, !self.channel_shown(id));
            self.display_revision = self.display_revision.wrapping_add(1);
        }
    }

    pub fn set_scroll(&mut self, top: i32, at_bottom: bool) {
        self.scroll_top = top.max(0);
        self.at_bottom = at_bottom;
    }

    pub fn draft_key(&self) -> String {
        self.identity
            .as_ref()
            .map(|identity| {
                format!(
                    "lot:chat:{}:{}:{}:{}:{}",
                    identity.source_epoch,
                    identity.lot_location,
                    identity.lot_incarnation,
                    identity.viewer_id,
                    identity.viewer_incarnation
                )
            })
            .unwrap_or_else(|| "lot:chat".into())
    }
}
