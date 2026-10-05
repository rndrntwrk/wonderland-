// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
// Translated from FreeSO VMEODTimerPlugin.cs; original FreeSO contributors.
use crate::{TimerRegisters, TimerVmEvent};

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Output {
    Public(TimerVmEvent),
    Binary(&'static str, Vec<u8>),
    Text(&'static str, String),
}

// Test diagnostics must redact outputs as well as externally visible types.
impl std::fmt::Debug for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TimerOutput([REDACTED])")
    }
}

#[derive(Clone)]
pub(crate) struct Timer {
    pub(crate) minutes: i16,
    pub(crate) seconds: i16,
    pub(crate) running: bool,
    pub(crate) mode: u8,
    pub(crate) updated_after_stop: bool,
    pub(crate) tock: i32,
}

impl Timer {
    pub(crate) fn connect(registers: TimerRegisters) -> (Self, Vec<Output>) {
        let [running, mode, minutes, seconds] = registers.0;
        (
            Self {
                minutes,
                seconds,
                running: running == 1,
                mode: if mode == 1 { 1 } else { 0 },
                updated_after_stop: true,
                tock: 0,
            },
            Self::show(registers),
        )
    }

    pub(crate) fn show(registers: TimerRegisters) -> Vec<Output> {
        vec![Output::Binary(
            "Timer_Show",
            registers.0.iter().map(|v| *v as u8).collect(),
        )]
    }

    pub(crate) fn binary(&mut self, event: &str, bytes: &[u8]) -> Vec<Output> {
        match (event, bytes) {
            ("Timer_State_Change", [mode @ 0..=1, ..]) if *mode != self.mode => {
                self.mode = *mode;
                vec![Output::Public(TimerVmEvent::ToggleStopwatch)]
            }
            ("Timer_IsRunning_Change", [running @ 0..=1, ..])
                if (*running == 1) != self.running =>
            {
                self.running = *running == 1;
                if self.running {
                    self.updated_after_stop = false;
                }
                vec![Output::Public(if self.running {
                    TimerVmEvent::Start
                } else {
                    TimerVmEvent::Pause
                })]
            }
            ("Timer_Set", [minutes @ 0..=99, seconds @ 0..=59, ..]) => {
                self.minutes = i16::from(*minutes) * 256;
                self.seconds = i16::from(*seconds);
                vec![Output::Public(TimerVmEvent::SetTime {
                    packed_time: self.minutes + self.seconds,
                })]
            }
            _ => vec![],
        }
    }

    pub(crate) fn tick(&mut self, registers: TimerRegisters) -> Vec<Output> {
        let mut output = vec![];
        if self.running {
            self.tock = 0;
            if self.mode == 0 && registers.0[2] == 0 && registers.0[3] == 0 {
                self.running = false;
                self.updated_after_stop = true;
                self.minutes = registers.0[2];
                self.seconds = registers.0[3];
                output.push(Output::Binary("Timer_Off", vec![self.mode]));
                output.push(self.update());
            }
        } else if !self.updated_after_stop {
            if self.tock == 0 {
                output.push(Output::Public(TimerVmEvent::Update));
            }
            // Preserve the C# unchecked Int32 behavior, including its immediate
            // start/pause quirk: only a running Tick resets Tock to zero.
            self.tock = self.tock.wrapping_add(1);
            if self.tock == 5 {
                self.updated_after_stop = true;
                self.minutes = registers.0[2];
                self.seconds = registers.0[3];
                output.push(self.update());
            }
        }
        output
    }

    fn update(&self) -> Output {
        Output::Text("Timer_Update", format!("{}:{}", self.minutes, self.seconds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Source: VMEODTimerPlugin.OnConnection; byte casts wrap even for invalid modes.
    #[test]
    fn connect_preserves_source_wire_bytes() {
        let (_, output) = Timer::connect(TimerRegisters([2, 257, 300, -1]));
        assert_eq!(
            output,
            vec![Output::Binary("Timer_Show", vec![2, 1, 44, 255])]
        );
    }

    // Source: SetTimerHandler packs minutes in the high byte and accepts trailing bytes.
    #[test]
    fn set_time_packs_minutes_exactly() {
        let (mut timer, _) = Timer::connect(TimerRegisters([0, 0, 0, 0]));
        assert_eq!(
            timer.binary("Timer_Set", &[99, 59, 222]),
            vec![Output::Public(TimerVmEvent::SetTime {
                packed_time: 99 * 256 + 59
            })]
        );
        assert!(timer.binary("Timer_Set", &[100, 0]).is_empty());
        assert!(timer.binary("Timer_Set", &[0, 60]).is_empty());
    }

    // Source: Tick emits Update on the first stopped tick and reads registers on tick five.
    #[test]
    fn pause_refreshes_after_exactly_five_ticks() {
        let (mut timer, _) = Timer::connect(TimerRegisters([0, 1, 1, 2]));
        assert_eq!(
            timer.binary("Timer_IsRunning_Change", &[1]),
            vec![Output::Public(TimerVmEvent::Start)]
        );
        timer.tick(TimerRegisters([1, 1, 1, 2]));
        assert_eq!(
            timer.binary("Timer_IsRunning_Change", &[0]),
            vec![Output::Public(TimerVmEvent::Pause)]
        );
        assert_eq!(
            timer.tick(TimerRegisters([0, 1, 1, 3])),
            vec![Output::Public(TimerVmEvent::Update)]
        );
        for seconds in 4..7 {
            assert!(timer.tick(TimerRegisters([0, 1, 1, seconds])).is_empty());
        }
        assert_eq!(
            timer.tick(TimerRegisters([0, 1, 1, 7])),
            vec![Output::Text("Timer_Update", "1:7".into())]
        );
        assert!(timer.tick(TimerRegisters([0, 1, 1, 8])).is_empty());
    }

    #[test]
    fn countdown_fallback_completes_but_stopwatch_does_not() {
        let (mut countdown, _) = Timer::connect(TimerRegisters([1, 257, 2, 3]));
        assert_eq!(
            countdown.tick(TimerRegisters([1, 257, 0, 0])),
            vec![
                Output::Binary("Timer_Off", vec![0]),
                Output::Text("Timer_Update", "0:0".into())
            ]
        );
        assert!(countdown.tick(TimerRegisters([1, 257, 0, 0])).is_empty());
        let (mut stopwatch, _) = Timer::connect(TimerRegisters([1, 1, 0, 0]));
        assert!(stopwatch.tick(TimerRegisters([1, 1, 0, 0])).is_empty());
        assert!(stopwatch.running);
    }

    #[test]
    fn toggles_are_transition_only_and_initial_running_pause_has_no_refresh() {
        let (mut timer, _) = Timer::connect(TimerRegisters([1, 0, 1, 2]));
        assert!(timer.binary("Timer_State_Change", &[0]).is_empty());
        assert_eq!(
            timer.binary("Timer_State_Change", &[1, 255]),
            vec![Output::Public(TimerVmEvent::ToggleStopwatch)]
        );
        assert!(timer.binary("Timer_State_Change", &[1]).is_empty());
        assert_eq!(
            timer.binary("Timer_State_Change", &[0]),
            vec![Output::Public(TimerVmEvent::ToggleStopwatch)]
        );
        assert!(timer.binary("Timer_IsRunning_Change", &[1]).is_empty());
        assert_eq!(
            timer.binary("Timer_IsRunning_Change", &[0]),
            vec![Output::Public(TimerVmEvent::Pause)]
        );
        // Constructor defaults UpdatedAfterStop=true even when initial args say running.
        for _ in 0..6 {
            assert!(timer.tick(TimerRegisters([0, 0, 1, 2])).is_empty());
        }
    }

    #[test]
    fn immediate_start_pause_retains_source_tock_quirk_instead_of_resetting() {
        let (mut timer, _) = Timer::connect(TimerRegisters([0, 1, 1, 2]));
        timer.binary("Timer_IsRunning_Change", &[1]);
        timer.tick(TimerRegisters([1, 1, 1, 2]));
        timer.binary("Timer_IsRunning_Change", &[0]);
        for _ in 0..5 {
            timer.tick(TimerRegisters([0, 1, 1, 2]));
        }
        assert_eq!(timer.tock, 5);
        timer.binary("Timer_IsRunning_Change", &[1]);
        timer.binary("Timer_IsRunning_Change", &[0]); // No intervening running Tick.
        for _ in 0..300 {
            assert!(timer.tick(TimerRegisters([0, 1, 1, 2])).is_empty());
        }
        assert_eq!(timer.tock, 305);
        assert!(!timer.updated_after_stop);
        timer.binary("Timer_IsRunning_Change", &[1]);
        timer.tick(TimerRegisters([1, 1, 1, 2]));
        timer.binary("Timer_IsRunning_Change", &[0]);
        assert_eq!(
            timer.tick(TimerRegisters([0, 1, 1, 2])),
            vec![Output::Public(TimerVmEvent::Update)]
        );
    }
}
