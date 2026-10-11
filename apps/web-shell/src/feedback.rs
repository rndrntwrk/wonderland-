//! Feedback for the current reply, independent of earlier operation errors.
use wonderland_client_app::ShellState;
use wonderland_contracts::{RequestKind, UiError, UiEvent, UiRequest};

#[derive(Debug, PartialEq, Eq)]
pub enum ReplyFeedback {
    Announcement(&'static str),
    TravelAnnouncement(String),
    Error(UiError),
    None,
}

/// Apply the current event without treating a retained earlier error as its outcome.
/// The bridge verifies exact pending-request identity before delivering fixture replies.
pub fn receive_reply(state: &mut ShellState, request: &UiRequest, event: UiEvent) -> ReplyFeedback {
    let feedback = match (&event, &request.kind) {
        (UiEvent::Rejected { reason, .. }, _) => {
            ReplyFeedback::Error(UiError::Rejected(reason.chars().take(256).collect()))
        }
        (UiEvent::Accepted { .. }, RequestKind::Travel { .. }) => {
            ReplyFeedback::TravelAnnouncement(crate::authoring_adapter::travel_message(
                state, request, false,
            ))
        }
        (UiEvent::Accepted { .. }, RequestKind::Interaction { .. }) => {
            ReplyFeedback::Announcement("Action accepted and added to the queue.")
        }
        (UiEvent::CancellationAcknowledged { .. }, RequestKind::Cancellation { .. }) => {
            ReplyFeedback::Announcement("Action cancelled.")
        }
        _ => ReplyFeedback::None,
    };
    match state.receive(event) {
        Ok(()) => feedback,
        Err(error) => ReplyFeedback::Error(error),
    }
}
