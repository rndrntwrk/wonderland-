# Timer source-derived expectations

These are **synthetic native test expectations**, derived from the source at
`4c6b3e8f5835b228723caea3c9f683c62f244f73`. They are not recordings from the C#
runtime, and do not establish original-runtime, UI, object-content or VM parity.

Source anchors:

- `TSOClient/tso.simantics/NetPlay/EODs/VMEODServer.cs:29-64`: source registration dictionary.
- `TSOClient/tso.client/UI/Panels/EODs/UIEODController.cs:13-46`: UI dictionary.
- `TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODTimerPlugin.cs:20-23`: allowlisted event/payload kinds.
- `VMEODTimerPlugin.cs:26-73`: Tick, stopped refresh and countdown completion.
- `VMEODTimerPlugin.cs:76-96`: OnConnection, raw source byte casts and internal mode default.
- `VMEODTimerPlugin.cs:103-131`: mode toggle behavior and value bounds.
- `VMEODTimerPlugin.cs:133-160`: start/pause transitions and UpdatedAfterStop.
- `VMEODTimerPlugin.cs:162-171`: time packing, bounds and minimum length.
- `VMEODTimerPlugin.cs:173-195`: close and public event codes.
- `TSOClient/tso.client/UI/Panels/EODs/UITimerPluginEOD.cs:57-75`: private event handlers, close and Timer_Set.
- `UITimerPluginEOD.cs:144-182`: Timer_Off and Timer_Update handling; the UI sends a Timer_Set echo after an update.

| Synthetic input / state | Source-derived expected output |
| --- | --- |
| Connect registers `[2, 257, 300, -1]` | Private binary `Timer_Show [2, 1, 44, 255]`; internal running false, mode countdown |
| `Timer_Set [99,59,222]` | VM event `2`, temp argument `25403`; trailing bytes ignored |
| `Timer_Set []`, `[1]`, `[100,59]`, `[99,60]` | No plugin output/state mutation |
| Start stopped timer | VM event `3`; `UpdatedAfterStop = false`; Tock is not reset by this handler |
| Pause running timer | VM event `4`; first subsequent stopped tick emits VM event `0` if Tock was zero |
| Stopped tick five with authoritative registers `[0,1,1,7]` | Private text `Timer_Update "1:7"`; no update on tick six |
| Running countdown observes register minutes/seconds zero | Private binary `Timer_Off [0]`, then private text `Timer_Update "0:0"`; no source VM pause event |
| Running stopwatch observes register minutes/seconds zero | No countdown completion output |
| Timer connected already running, then paused before any Start command | Source `UpdatedAfterStop` remains true; no five-tick refresh is requested |
| Refresh completed at Tock 5; Start and Pause arrive with no running tick between them | Tock is not reset; later stopped ticks pass 5 and do not produce a refresh until a running tick resets the counter |
| Close, plaintext payload of any bounded content | VM disconnect code `-1` and private `eod_leave`; stale session no longer usable |

The host additionally introduces explicit authentication, scoped session routing, epochs,
sequence numbers, queue limits, idle expiry and a new protocol envelope. Those
are native boundary policies, not assertions that the C# transport has them.
Only the native host contains timer state; the UI receives private outputs and
public VM integration receives only the separately typed object events.

Red-to-green evidence: the first three timer tests compiled against empty methods
and failed with zero outputs; the same tests pass after source translation. See
the Task 5 implementation report for complete commands and final test counts.
