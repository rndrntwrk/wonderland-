//! A directory/admission adapter boundary, not a directory or admission service.
use crate::{camera::CityCamera, Error};
use std::collections::BTreeSet;
use wonderland_render_core::FrameStamp;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DestinationId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryProvenance {
    Fixture,
    LiveProvider,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Destination {
    pub id: DestinationId,
    pub location: (u16, u16),
    pub revision: u64,
    pub available: bool,
    pub expected_lot_id: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct DirectorySnapshot {
    pub revision: u64,
    pub provenance: DirectoryProvenance,
    pub destinations: Vec<Destination>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CityIntent {
    /// Saved controls use city coordinates, never lot graphics coordinates.
    pub camera: CityCamera,
    pub selected: Option<DestinationId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestTicket {
    pub session_generation: u64,
    pub serial: u64,
    pub destination: DestinationId,
    pub directory_revision: u64,
    pub destination_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmissionReceipt {
    /// Echo the originating presentation request, including its session namespace.
    pub session_generation: u64,
    pub request_serial: u64,
    pub destination: DestinationId,
    pub directory_revision: u64,
    pub destination_revision: u64,
    pub lot_id: u64,
    pub epoch: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionState {
    City,
    AwaitingAdmission(RequestTicket),
    AwaitingFirstFrame {
        ticket: RequestTicket,
        receipt: AdmissionReceipt,
    },
    InLot {
        destination: DestinationId,
        lot_id: u64,
        epoch: u64,
    },
}
pub struct CityLotTransition {
    directory: DirectorySnapshot,
    intent: CityIntent,
    state: TransitionState,
    serial: u64,
    session_generation: u64,
}
impl DirectorySnapshot {
    pub fn validate(&self) -> Result<(), Error> {
        if self.destinations.len() > 65535 {
            return Err(Error::BudgetExceeded("directory records"));
        }
        let mut ids = BTreeSet::new();
        for d in &self.destinations {
            if d.id.0 == 0
                || d.revision == 0
                || d.location.0 >= 512
                || d.location.1 >= 512
                || d.expected_lot_id == Some(0)
                || !ids.insert(d.id)
            {
                return Err(Error::InvalidInput(
                    "directory destination identity/location",
                ));
            }
        }
        Ok(())
    }
    pub fn lookup(&self, id: DestinationId) -> Option<&Destination> {
        self.destinations.iter().find(|d| d.id == id)
    }
}
impl CityLotTransition {
    /// The transport/session owner must supply a nonzero generation that is never
    /// reused across reconnects or recreation of this controller. A serial is only
    /// unique within that generation; this object deliberately has no wall clock
    /// or process-global counter.
    pub fn new(
        directory: DirectorySnapshot,
        intent: CityIntent,
        session_generation: u64,
    ) -> Result<Self, Error> {
        if session_generation == 0 {
            return Err(Error::InvalidInput("transition session generation"));
        }
        directory.validate()?;
        intent.camera.pose()?.view_projection(1.)?;
        if intent
            .selected
            .map(|id| directory.lookup(id).is_none())
            .unwrap_or(false)
        {
            return Err(Error::InvalidInput("city selection"));
        }
        Ok(Self {
            directory,
            intent,
            state: TransitionState::City,
            serial: 0,
            session_generation,
        })
    }
    pub fn state(&self) -> TransitionState {
        self.state
    }
    pub fn provenance(&self) -> DirectoryProvenance {
        self.directory.provenance
    }
    pub fn begin_enter(&mut self, destination: DestinationId) -> Result<RequestTicket, Error> {
        if matches!(
            self.state,
            TransitionState::InLot { .. } | TransitionState::AwaitingFirstFrame { .. }
        ) {
            return Err(Error::InvalidInput(
                "return to city before entering another lot",
            ));
        }
        let record = self
            .directory
            .lookup(destination)
            .ok_or(Error::InvalidInput("unknown persistent destination"))?;
        if !record.available {
            return Err(Error::InvalidInput("unavailable destination"));
        }
        let serial = self
            .serial
            .checked_add(1)
            .ok_or(Error::BudgetExceeded("transition ticket sequence"))?;
        let ticket = RequestTicket {
            session_generation: self.session_generation,
            serial,
            destination,
            directory_revision: self.directory.revision,
            destination_revision: record.revision,
        };
        self.serial = serial;
        self.intent.selected = Some(destination);
        self.state = TransitionState::AwaitingAdmission(ticket);
        Ok(ticket)
    }
    pub fn admit(&mut self, ticket: RequestTicket, receipt: AdmissionReceipt) -> Result<(), Error> {
        if self.state != TransitionState::AwaitingAdmission(ticket)
            || ticket.directory_revision != self.directory.revision
        {
            return Err(Error::StaleTransition);
        }
        let record = self
            .directory
            .lookup(ticket.destination)
            .ok_or(Error::StaleTransition)?;
        if receipt.session_generation != ticket.session_generation
            || receipt.request_serial != ticket.serial
            || receipt.destination != ticket.destination
            || receipt.directory_revision != ticket.directory_revision
            || receipt.destination_revision != ticket.destination_revision
            || record.revision != ticket.destination_revision
            || !record.available
            || receipt.lot_id == 0
            || receipt.epoch == 0
            || record
                .expected_lot_id
                .map(|id| id != receipt.lot_id)
                .unwrap_or(false)
        {
            return Err(Error::InvalidInput("admission receipt identity"));
        }
        self.state = TransitionState::AwaitingFirstFrame { ticket, receipt };
        Ok(())
    }
    pub fn reject(&mut self, ticket: RequestTicket) -> Result<CityIntent, Error> {
        if self.state != TransitionState::AwaitingAdmission(ticket) {
            return Err(Error::StaleTransition);
        }
        self.state = TransitionState::City;
        Ok(self.intent)
    }
    /// The caller admits/validates the full core frame independently. A matching
    /// stamp proves routing consistency, not provider, engine, or service acceptance.
    pub fn present_first_frame(
        &mut self,
        ticket: RequestTicket,
        frame: FrameStamp,
    ) -> Result<(), Error> {
        let TransitionState::AwaitingFirstFrame {
            ticket: active,
            receipt,
        } = self.state
        else {
            return Err(Error::StaleTransition);
        };
        if active != ticket {
            return Err(Error::StaleTransition);
        }
        if frame.lot_id != receipt.lot_id || frame.epoch != receipt.epoch {
            return Err(Error::InvalidInput("first lot frame identity"));
        }
        self.state = TransitionState::InLot {
            destination: receipt.destination,
            lot_id: receipt.lot_id,
            epoch: receipt.epoch,
        };
        Ok(())
    }
    /// Returns the invalidated pending ticket so the transport owner can cancel
    /// its request/release an admission; this renderer performs no service writes.
    pub fn update_directory(
        &mut self,
        snapshot: DirectorySnapshot,
    ) -> Result<Option<RequestTicket>, Error> {
        snapshot.validate()?;
        if snapshot.revision <= self.directory.revision {
            return Err(Error::StaleTransition);
        }
        let canceled = match self.state {
            TransitionState::AwaitingAdmission(t)
            | TransitionState::AwaitingFirstFrame { ticket: t, .. } => Some(t),
            _ => None,
        };
        if self
            .intent
            .selected
            .map(|id| snapshot.lookup(id).is_none())
            .unwrap_or(false)
        {
            self.intent.selected = None;
        }
        self.directory = snapshot;
        if canceled.is_some() {
            self.state = TransitionState::City;
        }
        Ok(canceled)
    }
    pub fn return_to_city(&mut self) -> CityIntent {
        self.state = TransitionState::City;
        self.intent
    }
}
