use serde::{Deserialize, Serialize};

pub const LEAVE_LOT_ROUTINE: u16 = 8373;
pub const LEAVE_LOT_INTERACTION: u16 = 173;
pub const FORCE_DELETE_TIMEOUT: i32 = 60 * 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecyclePhase {
    Active,
    Leaving,
    DeleteRequested,
    Removed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvatarLifecycle {
    pub phase: LifecyclePhase,
    pub kill_timeout: i32,
    pub session_epoch: u64,
    pub connected: bool,
    pub disconnected_at: Option<u64>,
}
impl Default for AvatarLifecycle {
    fn default() -> Self {
        Self {
            phase: LifecyclePhase::Active,
            kill_timeout: -1,
            session_epoch: 0,
            connected: true,
            disconnected_at: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleError {
    StaleSession,
    AlreadyDeparting,
    InvalidContinuation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaveContext {
    pub action_available: bool,
    pub action_already_queued: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleRequest {
    CancelInteractions,
    EnqueueLeave {
        routine: u16,
        interaction: u16,
        skip_permissions: bool,
    },
    ForceEodDisconnect,
    ForceWaitTimeout {
        minimum: i32,
    },
    SetDisplayFlags(i16),
    SetHidden(i16),
    Delete {
        cleanup: bool,
    },
    RelocateAtMailbox,
}
impl AvatarLifecycle {
    pub fn validate(&self) -> Result<(), LifecycleError> {
        let timer_matches_phase = match self.phase {
            LifecyclePhase::Active => self.kill_timeout == -1,
            LifecyclePhase::Leaving => (0..=FORCE_DELETE_TIMEOUT).contains(&self.kill_timeout),
            LifecyclePhase::DeleteRequested => self.kill_timeout == FORCE_DELETE_TIMEOUT + 1,
            LifecyclePhase::Removed => !self.connected,
        };
        if self.kill_timeout < -1
            || self.kill_timeout > FORCE_DELETE_TIMEOUT + 1
            || (self.connected && self.disconnected_at.is_some())
            || !timer_matches_phase
        {
            return Err(LifecycleError::InvalidContinuation);
        }
        Ok(())
    }
    pub fn disconnect(&mut self, session_epoch: u64, tick: u64) -> Result<(), LifecycleError> {
        if session_epoch != self.session_epoch {
            return Err(LifecycleError::StaleSession);
        }
        if self.connected {
            self.connected = false;
            self.disconnected_at = Some(tick);
        }
        Ok(())
    }
    /// Network reconnection retains all behavior continuations. A fresh session
    /// epoch fences old sockets; an already issued leave cannot be revoked.
    pub fn reconnect(&mut self, session_epoch: u64) -> Result<(), LifecycleError> {
        if session_epoch <= self.session_epoch {
            return Err(LifecycleError::StaleSession);
        }
        if self.phase != LifecyclePhase::Active {
            return Err(LifecycleError::AlreadyDeparting);
        }
        self.session_epoch = session_epoch;
        self.connected = true;
        self.disconnected_at = None;
        Ok(())
    }
    pub fn request_leave(&mut self, context: LeaveContext) -> Vec<LifecycleRequest> {
        if matches!(
            self.phase,
            LifecyclePhase::Removed | LifecyclePhase::DeleteRequested
        ) || context.action_already_queued
        {
            return Vec::new();
        }
        self.phase = LifecyclePhase::Leaving;
        let mut requests = vec![LifecycleRequest::CancelInteractions];
        if context.action_available {
            requests.push(LifecycleRequest::EnqueueLeave {
                routine: LEAVE_LOT_ROUTINE,
                interaction: LEAVE_LOT_INTERACTION,
                skip_permissions: true,
            });
        } else {
            self.kill_timeout = FORCE_DELETE_TIMEOUT;
        }
        if self.kill_timeout == -1 {
            self.kill_timeout = 0;
        }
        requests
    }
    pub fn tick(&mut self, context: LeaveContext) -> Vec<LifecycleRequest> {
        if self.kill_timeout < 0
            || matches!(
                self.phase,
                LifecyclePhase::Removed | LifecyclePhase::DeleteRequested
            )
        {
            return Vec::new();
        }
        self.kill_timeout += 1;
        if self.kill_timeout > FORCE_DELETE_TIMEOUT {
            self.phase = LifecyclePhase::DeleteRequested;
            return vec![
                LifecycleRequest::ForceEodDisconnect,
                LifecycleRequest::Delete { cleanup: true },
            ];
        }
        let mut requests = vec![
            LifecycleRequest::SetDisplayFlags(1),
            LifecycleRequest::SetHidden(((self.kill_timeout % 30) / 15) as i16),
            LifecycleRequest::ForceWaitTimeout { minimum: 1_000_000 },
        ];
        requests.extend(self.request_leave(context));
        requests
    }
    pub fn mark_removed(&mut self) {
        self.phase = LifecyclePhase::Removed;
        self.connected = false;
    }
}
