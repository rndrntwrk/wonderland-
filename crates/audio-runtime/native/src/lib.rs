//! Continuous, presentation-only native audio transport.
//!
//! The worker owns [`NativeMixer`] and all allocation, command processing and
//! completion delivery. The single device callback uses a preallocated atomic
//! frame ring: no locks, channel operations, allocation, decoding or mixing.
#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use wonderland_audio_runtime::mixer::{MixerIntent, VoiceId};
use wonderland_audio_runtime::pcm::{NativeMixer, PcmBuffer};
use wonderland_audio_runtime::AudioError;
use wonderland_render_core::AssetKey;

#[cfg(not(target_has_atomic = "64"))]
compile_error!("native audio requires native 64-bit atomic operations");

static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverConfig {
    pub sample_rate: u32,
    pub max_voices: usize,
    pub max_pcm_bytes: usize,
    pub max_queued_pcm_bytes: usize,
    pub command_capacity: usize,
    pub event_capacity: usize,
    pub block_frames: usize,
    pub ring_frames: usize,
    pub max_callback_frames: usize,
}
impl Default for DriverConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            max_voices: 128,
            max_pcm_bytes: 64 * 1024 * 1024,
            max_queued_pcm_bytes: 16 * 1024 * 1024,
            command_capacity: 64,
            event_capacity: 256,
            block_frames: 256,
            ring_frames: 2048,
            max_callback_frames: 16_384,
        }
    }
}
impl DriverConfig {
    pub fn validate(&self) -> Result<(), DriverError> {
        if !(1..=384_000).contains(&self.sample_rate)
            || !(1..=4096).contains(&self.max_voices)
            || !(2..=512 * 1024 * 1024).contains(&self.max_pcm_bytes)
            || !(2..=512 * 1024 * 1024).contains(&self.max_queued_pcm_bytes)
            || !(1..=1024).contains(&self.command_capacity)
            || !(1..=4096).contains(&self.event_capacity)
            || !(1..=65_536).contains(&self.ring_frames)
            // A polling worker needs scheduling headroom. Rings shorter than
            // two milliseconds are outside this adapter's supported policy.
            || self.ring_frames as u64 * 500 < u64::from(self.sample_rate)
            || self.block_frames == 0
            || self.block_frames > self.ring_frames
            || self.block_frames > self.sample_rate as usize
            || !(1..=65_536).contains(&self.max_callback_frames)
        {
            return Err(DriverError::InvalidConfig);
        }
        Ok(())
    }
    fn poll_interval(&self) -> Duration {
        // At most one quarter of the ring duration, capped at one millisecond.
        // Config validation keeps this at least 500 microseconds. This is a
        // scheduling request, not a hard real-time deadline guarantee.
        let nanos = (self.ring_frames as u64 * 1_000_000_000 / u64::from(self.sample_rate) / 4)
            .min(1_000_000);
        Duration::from_nanos(nanos)
    }
}

/// A token from one controller instance and one reset generation. Neither field
/// is caller-mutable, so asynchronous asset loads cannot retag stale commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Session {
    instance: u64,
    generation: u64,
}
impl Session {
    pub fn instance(self) -> u64 {
        self.instance
    }
    pub fn generation(self) -> u64 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket(pub u64);

#[derive(Debug)]
pub enum Command {
    InsertSample { key: AssetKey, pcm: PcmBuffer },
    Apply(MixerIntent),
    EvictSample { key: AssetKey },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DeviceFault {
    Disconnected = 1,
    Backend = 2,
    CallbackShape = 3,
    CallbackTooLarge = 4,
    CounterExhausted = 5,
    Mixer = 6,
}
impl DeviceFault {
    fn decode(value: u8) -> Option<Self> {
        Some(match value {
            1 => Self::Disconnected,
            2 => Self::Backend,
            3 => Self::CallbackShape,
            4 => Self::CallbackTooLarge,
            5 => Self::CounterExhausted,
            6 => Self::Mixer,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriverError {
    InvalidConfig,
    StaleSession,
    Closed,
    CommandQueueFull,
    QueuedPcmLimit,
    CompletionBackpressure,
    CounterExhausted,
    DeviceFault(DeviceFault),
    Audio(AudioError),
}
impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DriverError {}
impl From<AudioError> for DriverError {
    fn from(error: AudioError) -> Self {
        Self::Audio(error)
    }
}

#[derive(Debug)]
pub struct SubmitError {
    pub kind: DriverError,
    /// Ownership is returned, including PCM, so backpressure does not force a
    /// second decode or a copy of a large sample.
    pub command: Command,
}
impl std::fmt::Display for SubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}
impl std::error::Error for SubmitError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Command {
        session: Session,
        ticket: Ticket,
        result: Result<(), DriverError>,
    },
    /// Conservative completion at the end of the consumed mixing block, not at
    /// production time. This can feed AudioSystem::complete_voice only.
    Finished { session: Session, voice: VoiceId },
}
impl Event {
    pub fn session(&self) -> Session {
        match self {
            Self::Command { session, .. } | Self::Finished { session, .. } => *session,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportState {
    Running,
    Suspended,
    Faulted,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub session: Session,
    pub state: TransportState,
    pub fault: Option<DeviceFault>,
    pub callbacks: u64,
    pub requested_frames: u64,
    pub copied_frames: u64,
    pub underrun_frames: u64,
    pub discarded_frames: u64,
    pub device_errors: u64,
    pub queued_frames: usize,
    pub queued_pcm_bytes: usize,
}

struct Slot {
    pcm: AtomicU32,
    epoch: AtomicU64,
}
struct Ring {
    slots: Box<[Slot]>,
    read: AtomicU64,
    write: AtomicU64,
}
impl Ring {
    fn new(capacity: usize) -> Self {
        Self {
            slots: (0..capacity)
                .map(|_| Slot {
                    pcm: AtomicU32::new(0),
                    epoch: AtomicU64::new(0),
                })
                .collect(),
            read: AtomicU64::new(0),
            write: AtomicU64::new(0),
        }
    }
    fn free(&self) -> usize {
        let write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        self.slots.len() - (write - read) as usize
    }
    fn publish(&self, pcm: &[i16], epoch: u64) -> Result<u64, DriverError> {
        let write = self.write.load(Ordering::Relaxed);
        let end = write
            .checked_add((pcm.len() / 2) as u64)
            .ok_or(DriverError::CounterExhausted)?;
        for (offset, frame) in pcm.chunks_exact(2).enumerate() {
            let index = ((write + offset as u64) % self.slots.len() as u64) as usize;
            let packed = u32::from(frame[0] as u16) | (u32::from(frame[1] as u16) << 16);
            self.slots[index].pcm.store(packed, Ordering::Relaxed);
            self.slots[index].epoch.store(epoch, Ordering::Relaxed);
        }
        // Publishing the tail makes every preceding frame and tag visible to
        // the consumer. Slots cannot be reused before its release of `read`.
        self.write.store(end, Ordering::Release);
        Ok(end)
    }
}

struct Shared {
    config: DriverConfig,
    instance: u64,
    generation: AtomicU64,
    epoch: AtomicU64,
    running: AtomicBool,
    closed: AtomicBool,
    expected: AtomicBool,
    fault: AtomicU8,
    queued_pcm_bytes: AtomicUsize,
    callbacks: AtomicU64,
    requested: AtomicU64,
    copied: AtomicU64,
    underrun: AtomicU64,
    discarded: AtomicU64,
    device_errors: AtomicU64,
    ring: Ring,
}
impl Shared {
    fn session(&self) -> Session {
        Session {
            instance: self.instance,
            generation: self.generation.load(Ordering::Acquire),
        }
    }
    fn fault(&self) -> Option<DeviceFault> {
        DeviceFault::decode(self.fault.load(Ordering::Acquire))
    }
    fn report_fault(&self, fault: DeviceFault) {
        self.device_errors.fetch_add(1, Ordering::Relaxed);
        let _ = self
            .fault
            .compare_exchange(0, fault as u8, Ordering::AcqRel, Ordering::Relaxed);
    }
    fn can_render(&self) -> bool {
        self.running.load(Ordering::Acquire)
            && !self.closed.load(Ordering::Acquire)
            && self.fault.load(Ordering::Acquire) == 0
    }
    fn invalidate(&self) -> Result<(), DriverError> {
        self.epoch
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                old.checked_add(1)
            })
            .map(|_| ())
            .map_err(|_| {
                self.report_fault(DeviceFault::CounterExhausted);
                DriverError::CounterExhausted
            })
    }
    fn snapshot(&self) -> Snapshot {
        let fault = self.fault();
        let state = if self.closed.load(Ordering::Acquire) {
            TransportState::Closed
        } else if fault.is_some() {
            TransportState::Faulted
        } else if self.running.load(Ordering::Acquire) {
            TransportState::Running
        } else {
            TransportState::Suspended
        };
        // Reading the consumer first gives a conservative, bounded snapshot
        // even if it and the producer move between these two loads.
        let read = self.ring.read.load(Ordering::Acquire);
        let write = self.ring.write.load(Ordering::Acquire);
        Snapshot {
            session: self.session(),
            state,
            fault,
            callbacks: self.callbacks.load(Ordering::Relaxed),
            requested_frames: self.requested.load(Ordering::Relaxed),
            copied_frames: self.copied.load(Ordering::Relaxed),
            underrun_frames: self.underrun.load(Ordering::Relaxed),
            discarded_frames: self.discarded.load(Ordering::Relaxed),
            device_errors: self.device_errors.load(Ordering::Relaxed),
            queued_frames: write
                .saturating_sub(read)
                .min(self.config.ring_frames as u64) as usize,
            queued_pcm_bytes: self.queued_pcm_bytes.load(Ordering::Relaxed),
        }
    }
}

/// Clone this handle on the control thread, then move it into the backend error
/// callback. Reporting does a fixed number of atomic operations and no logging.
#[derive(Clone)]
pub struct FaultSignal {
    shared: Arc<Shared>,
}
impl FaultSignal {
    pub fn report(&self, fault: DeviceFault) {
        self.shared.report_fault(fault);
    }
}

struct Request {
    session: Session,
    ticket: Ticket,
    command: Command,
    pcm_bytes: usize,
}

/// Single command producer and event consumer. All methods belong on a control
/// or application thread, never on the device callback thread.
pub struct Control {
    shared: Arc<Shared>,
    requests: SyncSender<Request>,
    events: Receiver<Event>,
    next_ticket: u64,
}
impl Control {
    pub fn session(&self) -> Session {
        self.shared.session()
    }
    pub fn snapshot(&self) -> Snapshot {
        self.shared.snapshot()
    }
    pub fn fault_signal(&self) -> FaultSignal {
        FaultSignal {
            shared: self.shared.clone(),
        }
    }
    pub fn try_submit(
        &mut self,
        session: Session,
        mut command: Command,
    ) -> Result<Ticket, SubmitError> {
        let invalid = if session != self.session() {
            Some(DriverError::StaleSession)
        } else if self.shared.closed.load(Ordering::Acquire) {
            Some(DriverError::Closed)
        } else if let Some(fault) = self.shared.fault() {
            Some(DriverError::DeviceFault(fault))
        } else if self.next_ticket == u64::MAX {
            Some(DriverError::CounterExhausted)
        } else {
            None
        };
        if let Some(kind) = invalid {
            return Err(SubmitError { kind, command });
        }
        let pcm_bytes = match &mut command {
            Command::InsertSample { pcm, .. } => {
                if let Err(error) = pcm.validate(self.shared.config.max_pcm_bytes) {
                    return Err(SubmitError {
                        kind: DriverError::Audio(error),
                        command,
                    });
                }
                // Account for the allocation, not just initialized elements.
                // Normalize ownership before NativeMixer measures residency by
                // length; otherwise a one-frame Vec could retain huge capacity.
                let allocated = pcm.samples.capacity().checked_mul(2);
                if allocated.map_or(true, |bytes| bytes > self.shared.config.max_pcm_bytes) {
                    return Err(SubmitError {
                        kind: DriverError::Audio(AudioError::Limit("PCM allocation")),
                        command,
                    });
                }
                pcm.samples = std::mem::take(&mut pcm.samples)
                    .into_boxed_slice()
                    .into_vec();
                pcm.samples.len() * 2
            }
            _ => 0,
        };
        if self
            .shared
            .queued_pcm_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                old.checked_add(pcm_bytes)
                    .filter(|new| *new <= self.shared.config.max_queued_pcm_bytes)
            })
            .is_err()
        {
            return Err(SubmitError {
                kind: DriverError::QueuedPcmLimit,
                command,
            });
        }
        let ticket = Ticket(self.next_ticket);
        let request = Request {
            session,
            ticket,
            command,
            pcm_bytes,
        };
        match self.requests.try_send(request) {
            Ok(()) => {
                self.next_ticket += 1;
                Ok(ticket)
            }
            Err(error) => {
                let (kind, request) = match error {
                    TrySendError::Full(request) => (DriverError::CommandQueueFull, request),
                    TrySendError::Disconnected(request) => (DriverError::Closed, request),
                };
                self.shared
                    .queued_pcm_bytes
                    .fetch_sub(request.pcm_bytes, Ordering::AcqRel);
                Err(SubmitError {
                    kind,
                    command: request.command,
                })
            }
        }
    }
    /// A reset cancels all earlier tickets and completion events. A bounded
    /// drain avoids looping forever if the worker publishes concurrently.
    pub fn try_event(&mut self) -> Option<Event> {
        for _ in 0..self.shared.config.event_capacity {
            match self.events.try_recv() {
                Ok(event) if event.session() == self.session() => return Some(event),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
        None
    }
    /// Takes effect at the next callback entry. A callback already copying a
    /// block completes it normally; queued, unconsumed frames remain intact.
    pub fn suspend(&mut self) {
        self.shared.running.store(false, Ordering::Release);
    }
    pub fn resume(&mut self) -> Result<(), DriverError> {
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(DriverError::Closed);
        }
        if let Some(fault) = self.shared.fault() {
            return Err(DriverError::DeviceFault(fault));
        }
        self.shared.running.store(true, Ordering::Release);
        if let Some(fault) = self.shared.fault() {
            return Err(DriverError::DeviceFault(fault));
        }
        Ok(())
    }
    /// Invalidates buffered output immediately and clears mixer residency on the
    /// next worker step. A callback already handed to hardware cannot be recalled.
    pub fn reset(&mut self) -> Result<Session, DriverError> {
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(DriverError::Closed);
        }
        self.shared
            .generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                old.checked_add(1)
            })
            .map_err(|_| DriverError::CounterExhausted)?;
        self.shared.invalidate()?;
        self.shared.expected.store(false, Ordering::Release);
        Ok(self.session())
    }
    pub fn stop(&mut self) -> Result<Session, DriverError> {
        self.suspend();
        self.reset()
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
    }
}

struct PendingCompletion {
    session: Session,
    voice: VoiceId,
    end: u64,
}

/// The sole ring producer. `step` supports deterministic testing or an existing
/// application audio thread; `spawn` creates a dedicated polling worker.
pub struct Worker {
    shared: Arc<Shared>,
    requests: Receiver<Request>,
    events: SyncSender<Event>,
    mixer: NativeMixer,
    known_session: Session,
    voice_generation: Option<u64>,
    reply: Option<Event>,
    completed: VecDeque<PendingCompletion>,
}
impl Worker {
    pub fn resident_bytes(&self) -> usize {
        self.mixer.resident_bytes()
    }
    pub fn active_voices(&self) -> usize {
        self.mixer.active_voices()
    }
    pub fn pending_completions(&self) -> usize {
        self.completed.len()
    }
    fn send_reply(&mut self) -> bool {
        if let Some(event) = self.reply.take() {
            if event.session() != self.shared.session() {
                return true;
            }
            match self.events.try_send(event) {
                Ok(()) => (),
                Err(TrySendError::Full(event)) => {
                    self.reply = Some(event);
                    return false;
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.shared.closed.store(true, Ordering::Release);
                    return false;
                }
            }
        }
        true
    }
    fn send_completions(&mut self) {
        // The start acknowledgement precedes its completion. Faulted output is
        // abandoned by reopening, never described as naturally completed.
        if self.reply.is_some() || self.shared.fault().is_some() {
            return;
        }
        let read = self.shared.ring.read.load(Ordering::Acquire);
        while self
            .completed
            .front()
            .map_or(false, |front| front.end <= read)
        {
            let pending = self.completed.pop_front().expect("front checked");
            if pending.session != self.shared.session() {
                continue;
            }
            let event = Event::Finished {
                session: pending.session,
                voice: pending.voice,
            };
            match self.events.try_send(event) {
                Ok(()) => (),
                Err(TrySendError::Full(_)) => {
                    self.completed.push_front(pending);
                    break;
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.shared.closed.store(true, Ordering::Release);
                    break;
                }
            }
        }
    }
    fn apply(&mut self, command: Command) -> Result<(), DriverError> {
        match command {
            Command::InsertSample { key, pcm } => {
                self.mixer.insert_sample(key, pcm).map_err(Into::into)
            }
            Command::EvictSample { key } => {
                self.mixer.evict_sample(key).map(|_| ()).map_err(Into::into)
            }
            Command::Apply(intent) => {
                let is_new_generation = match &intent {
                    MixerIntent::Start { voice, .. } => self
                        .voice_generation
                        .map_or(false, |old| voice.generation > old),
                    _ => false,
                };
                if matches!(intent, MixerIntent::Start { .. })
                    && !is_new_generation
                    && self.mixer.active_voices() + self.completed.len()
                        >= self.shared.config.max_voices
                {
                    return Err(DriverError::CompletionBackpressure);
                }
                self.mixer.apply(&intent)?;
                match intent {
                    MixerIntent::Start { voice, .. } => {
                        if is_new_generation {
                            self.shared.invalidate()?;
                            self.completed.clear();
                        }
                        self.voice_generation = Some(voice.generation);
                    }
                    MixerIntent::Stop { voice } | MixerIntent::Release { voice } => {
                        self.completed.retain(|pending| pending.voice != voice);
                    }
                    _ => (),
                }
                Ok(())
            }
        }
    }
    pub fn step(&mut self) {
        if self.shared.closed.load(Ordering::Acquire) {
            return;
        }
        let session = self.shared.session();
        if session != self.known_session {
            self.mixer.reset();
            self.completed.clear();
            self.reply = None;
            self.voice_generation = None;
            self.known_session = session;
        }
        self.send_completions();
        for _ in 0..self.shared.config.command_capacity {
            if !self.send_reply() {
                break;
            }
            let request = match self.requests.try_recv() {
                Ok(request) => request,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.shared.closed.store(true, Ordering::Release);
                    break;
                }
            };
            let pcm_bytes = request.pcm_bytes;
            if request.session != self.known_session || request.session != self.shared.session() {
                drop(request);
                self.shared
                    .queued_pcm_bytes
                    .fetch_sub(pcm_bytes, Ordering::AcqRel);
                continue;
            }
            let result = if let Some(fault) = self.shared.fault() {
                drop(request.command);
                Err(DriverError::DeviceFault(fault))
            } else {
                self.apply(request.command)
            };
            self.shared
                .queued_pcm_bytes
                .fetch_sub(pcm_bytes, Ordering::AcqRel);
            self.reply = Some(Event::Command {
                session: request.session,
                ticket: request.ticket,
                result,
            });
        }
        self.send_reply();
        self.shared
            .expected
            .store(self.mixer.active_voices() != 0, Ordering::Release);
        if self.shared.can_render()
            && self.known_session == self.shared.session()
            && self.mixer.active_voices() != 0
            && self.shared.ring.free() >= self.shared.config.block_frames
        {
            let epoch = self.shared.epoch.load(Ordering::Acquire);
            match self.mixer.render(self.shared.config.block_frames) {
                Ok(pcm) => {
                    let finished = self.mixer.take_finished();
                    if self.known_session == self.shared.session()
                        && epoch == self.shared.epoch.load(Ordering::Acquire)
                    {
                        match self.shared.ring.publish(&pcm, epoch) {
                            Ok(end) => {
                                for voice in finished {
                                    // A start reserves space against active +
                                    // completed, so this fixed bound is invariant.
                                    self.completed.push_back(PendingCompletion {
                                        session: self.known_session,
                                        voice,
                                        end,
                                    });
                                }
                            }
                            Err(_) => self.shared.report_fault(DeviceFault::CounterExhausted),
                        }
                    }
                }
                Err(_) => self.shared.report_fault(DeviceFault::Mixer),
            }
        }
        self.shared
            .expected
            .store(self.mixer.active_voices() != 0, Ordering::Release);
        self.send_completions();
    }
    /// Process a bounded refill burst. At most one ring's worth of blocks is
    /// produced, even if a fast virtual device keeps consuming concurrently.
    /// This avoids imposing a fixed 1,000-block/second mixer ceiling.
    pub fn refill(&mut self) -> usize {
        let mut produced = 0;
        let limit = self.shared.config.ring_frames / self.shared.config.block_frames;
        for _ in 0..limit {
            let before = self.shared.ring.write.load(Ordering::Relaxed);
            self.step();
            let after = self.shared.ring.write.load(Ordering::Relaxed);
            if after == before {
                break;
            }
            produced += (after - before) as usize;
        }
        produced
    }
    pub fn spawn(mut self) -> std::io::Result<WorkerThread> {
        let shared = self.shared.clone();
        let handle = thread::Builder::new()
            .name("wonderland-audio-mixer".into())
            .spawn(move || {
                while !self.shared.closed.load(Ordering::Acquire) {
                    self.refill();
                    // Scheduling and all waits are off the device callback. No
                    // callback-side syscall is needed to wake the producer.
                    thread::park_timeout(self.shared.config.poll_interval());
                }
            })?;
        Ok(WorkerThread {
            shared,
            handle: Some(handle),
        })
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
        for request in self.requests.try_iter() {
            self.shared
                .queued_pcm_bytes
                .fetch_sub(request.pcm_bytes, Ordering::AcqRel);
        }
    }
}

pub struct WorkerThread {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}
impl WorkerThread {
    /// Join belongs on the control thread, after dropping the native stream.
    pub fn shutdown(mut self) -> thread::Result<()> {
        self.shared.closed.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            handle.thread().unpark();
            handle.join()
        } else {
            Ok(())
        }
    }
}
impl Drop for WorkerThread {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            handle.thread().unpark();
            let _ = handle.join();
        }
    }
}

trait OutputSample: Copy {
    fn silence() -> Self;
    fn from_pcm(sample: i16) -> Self;
}
impl OutputSample for i16 {
    fn silence() -> Self {
        0
    }
    fn from_pcm(sample: i16) -> Self {
        sample
    }
}
impl OutputSample for f32 {
    fn silence() -> Self {
        0.0
    }
    fn from_pcm(sample: i16) -> Self {
        f32::from(sample) / 32768.0
    }
}
impl OutputSample for u16 {
    fn silence() -> Self {
        32768
    }
    fn from_pcm(sample: i16) -> Self {
        (i32::from(sample) + 32768) as u16
    }
}

/// Move this unique endpoint into the backend FnMut data callback. It contains
/// no application callbacks and never owns sample Vecs or completion messages.
pub struct Callback {
    shared: Arc<Shared>,
}
impl Callback {
    pub fn write_i16(&mut self, output: &mut [i16]) {
        self.write(output);
    }
    pub fn write_f32(&mut self, output: &mut [f32]) {
        self.write(output);
    }
    pub fn write_u16(&mut self, output: &mut [u16]) {
        self.write(output);
    }
    fn write<T: OutputSample>(&mut self, output: &mut [T]) {
        self.write_before_commit(output, || {});
    }
    // The private no-op seam lets unit tests place a control transition exactly
    // between copy and commit. Device integrations cannot supply a callback.
    fn write_before_commit<T: OutputSample>(
        &mut self,
        output: &mut [T],
        before_commit: impl FnOnce(),
    ) {
        output.fill(T::silence());
        self.shared.callbacks.fetch_add(1, Ordering::Relaxed);
        let frames = output.len() / 2;
        self.shared
            .requested
            .fetch_add(frames as u64, Ordering::Relaxed);
        if output.len() % 2 != 0 {
            self.shared.report_fault(DeviceFault::CallbackShape);
            return;
        }
        if frames > self.shared.config.max_callback_frames {
            self.shared.report_fault(DeviceFault::CallbackTooLarge);
            return;
        }
        if !self.shared.can_render() {
            return;
        }
        let epoch = self.shared.epoch.load(Ordering::Acquire);
        let mut read = self.shared.ring.read.load(Ordering::Relaxed);
        // One snapshot bounds examination to at most ring_frames slots, even if
        // the producer keeps publishing during this invocation.
        let end = self.shared.ring.write.load(Ordering::Acquire);
        let mut copied = 0;
        let mut discarded = 0;
        while read < end && copied < frames {
            let index = (read % self.shared.ring.slots.len() as u64) as usize;
            let slot = &self.shared.ring.slots[index];
            if slot.epoch.load(Ordering::Relaxed) != epoch {
                discarded += 1;
            } else {
                let pcm = slot.pcm.load(Ordering::Relaxed);
                output[copied * 2] = T::from_pcm(pcm as u16 as i16);
                output[copied * 2 + 1] = T::from_pcm((pcm >> 16) as u16 as i16);
                copied += 1;
            }
            read += 1;
        }
        before_commit();
        if epoch != self.shared.epoch.load(Ordering::Acquire)
            || self.shared.closed.load(Ordering::Acquire)
            || self.shared.fault.load(Ordering::Acquire) != 0
        {
            // Reset/fault may race this copy. Suspension is linearized at entry
            // and deliberately does not discard a block already in progress.
            output.fill(T::silence());
            discarded += copied;
            copied = 0;
        }
        self.shared.ring.read.store(read, Ordering::Release);
        self.shared
            .copied
            .fetch_add(copied as u64, Ordering::Relaxed);
        self.shared
            .discarded
            .fetch_add(discarded as u64, Ordering::Relaxed);
        if self.shared.can_render() && self.shared.expected.load(Ordering::Acquire) {
            self.shared
                .underrun
                .fetch_add((frames - copied) as u64, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod commit_races {
    use super::*;
    use wonderland_audio_runtime::mixer::VolumeGroup;

    fn ready() -> (Control, Worker, Callback) {
        let config = DriverConfig {
            sample_rate: 8,
            block_frames: 2,
            ring_frames: 4,
            ..DriverConfig::default()
        };
        let (mut control, mut worker, callback) = channel(config).unwrap();
        let session = control.session();
        control
            .try_submit(
                session,
                Command::InsertSample {
                    key: AssetKey([1; 32]),
                    pcm: PcmBuffer {
                        sample_rate: 8,
                        channels: 1,
                        samples: vec![11, 22, 33, 44],
                    },
                },
            )
            .unwrap();
        control
            .try_submit(
                session,
                Command::Apply(MixerIntent::Start {
                    voice: VoiceId {
                        generation: 1,
                        serial: 1,
                    },
                    sample: AssetKey([1; 32]),
                    group: VolumeGroup::Fx,
                    gain: 1.0,
                    pan: 0.0,
                    looped: false,
                    seek_frame: 0,
                }),
            )
            .unwrap();
        worker.step();
        (control, worker, callback)
    }

    #[test]
    fn suspension_during_commit_delivers_one_block_and_preserves_next_block() {
        let (mut control, mut worker, mut callback) = ready();
        worker.step();
        let mut out = [0; 4];
        callback.write_before_commit(&mut out, || control.suspend());
        assert_eq!(out, [11, 11, 22, 22]);
        callback.write_i16(&mut out);
        assert_eq!(out, [0; 4]);
        assert_eq!(control.snapshot().queued_frames, 2);
        control.resume().unwrap();
        callback.write_i16(&mut out);
        assert_eq!(out, [33, 33, 44, 44]);
    }

    #[test]
    fn reset_during_commit_discards_the_entire_old_block() {
        let (mut control, mut worker, mut callback) = ready();
        let mut out = [0i16; 4];
        callback.write_before_commit(&mut out, || {
            control.reset().unwrap();
        });
        assert_eq!(out, [0; 4]);
        worker.step();
        assert!(control.try_event().is_none());
        assert_eq!(control.snapshot().discarded_frames, 2);
    }

    #[test]
    fn device_fault_during_commit_discards_without_natural_completion() {
        let (mut control, mut worker, mut callback) = ready();
        worker.step();
        let fault = control.fault_signal();
        let mut out = [0i16; 8];
        callback.write_before_commit(&mut out, || fault.report(DeviceFault::Disconnected));
        assert_eq!(out, [0; 8]);
        worker.step();
        assert!(std::iter::from_fn(|| control.try_event())
            .all(|e| !matches!(e, Event::Finished { .. })));
        assert_eq!(worker.pending_completions(), 1);
    }
}

pub fn channel(config: DriverConfig) -> Result<(Control, Worker, Callback), DriverError> {
    config.validate()?;
    let instance = NEXT_INSTANCE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            old.checked_add(1)
        })
        .map_err(|_| DriverError::CounterExhausted)?;
    let mixer = NativeMixer::new(config.sample_rate, config.max_voices, config.max_pcm_bytes)?;
    let (request_tx, request_rx) = mpsc::sync_channel(config.command_capacity);
    let (event_tx, event_rx) = mpsc::sync_channel(config.event_capacity);
    let max_voices = config.max_voices;
    let ring = Ring::new(config.ring_frames);
    let shared = Arc::new(Shared {
        config,
        instance,
        ring,
        generation: AtomicU64::new(1),
        epoch: AtomicU64::new(1),
        running: AtomicBool::new(true),
        closed: AtomicBool::new(false),
        expected: AtomicBool::new(false),
        fault: AtomicU8::new(0),
        queued_pcm_bytes: AtomicUsize::new(0),
        callbacks: AtomicU64::new(0),
        requested: AtomicU64::new(0),
        copied: AtomicU64::new(0),
        underrun: AtomicU64::new(0),
        discarded: AtomicU64::new(0),
        device_errors: AtomicU64::new(0),
    });
    Ok((
        Control {
            shared: shared.clone(),
            requests: request_tx,
            events: event_rx,
            next_ticket: 1,
        },
        Worker {
            known_session: shared.session(),
            shared: shared.clone(),
            requests: request_rx,
            events: event_tx,
            mixer,
            voice_generation: None,
            reply: None,
            completed: VecDeque::with_capacity(max_voices),
        },
        Callback { shared },
    ))
}
