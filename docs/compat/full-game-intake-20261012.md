# Full-game source intake, 12 October 2026

Read-only source/census inspection accompanying the initial preservation ledger.
This is not complete ingestion, original-runtime parity, or gameplay acceptance.
The original reference remains `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

## Reusable evidence

- `docs/compat/baseline.json`: 47 solution projects, 40 present and 7 submodule-backed; 1,977 explicit Compile paths, 178 ProjectReference declarations and 63 imports. Existing read-only baseline gate passed with digest `990054a2101f863f61760a39b8efffc15aa66bb08717373e5feaff20bf4b46ee`.
- `fixtures/eod/registration-census.json`: 30 server / 28 UI registrations. Existing `tools/swarm-b/eod-census.py --check` passed. All 30 native translations exist; original-runtime and production-provider qualified counts remain zero.
- `docs/design/action-first/player-capability-map.md`: 21 preserved surfaces. `docs/compat/full-game-source-obligations.json`: 20 additional domain obligations. All referenced original files are present; abbreviated player-map filenames are resolved explicitly in the new ledger.
- `docs/compat/content-corpus.json`: 751 source files, including 745 IFF/PIFF, 1,327 decoded objects, 7,200 BHAVs and 805 TTABs. Effective-content identities remain unresolved.
- `docs/compat/object-matrix.json`: 1,602 leaf rows (1,327 objects and 275 source-only cohorts), 730 distinct source hashes. No full-installation baseline or gameplay-qualified row is asserted.
- `docs/compat/tools.json`: 11 tool and 3 optional-extension dispositions; not complete creator parity.

A fresh filtered fetch from the original repository on 12 October 2026 resolved
commit `4c6b3e8f5835b228723caea3c9f683c62f244f73` and tree
`ee7bd11065897bb6e00abb025777e0f72c2480d0`. Direct Git-blob comparison covers
all 5,466 original blobs: 5,465 match, no files are missing, and only the declared
fork `.gitignore` metadata differs. The two original gitlinks remain explicit
unresolved submodule obligations. This closes original-object availability and
preservation, not runtime parity or complete registration intake. Reproduce with
`tools/completion/verify-source-pin.py`; see the recorded original-pin report.

## Next registry cohorts

| Cohort | Original source anchor | Observed registration accounting |
| --- | --- | --- |
| Primitives | `TSOClient/tso.simantics/VMContext.cs::InitVMConfig` | 56 statements: 48 common, 6 TS1, 2 TSO; final maps 50 TSO and 53 TS1 because TS1 overrides opcode 30 |
| VM commands | `TSOClient/tso.simantics/NetPlay/Model/VMNetCommand.cs::CmdMap` | 49 IDs, 0–48; retain TS1/direct-control/SM64 dispositions |
| Transport | `TSOClient/FSO.Server.Protocol/{Aries,Voltron,Electron,Gluon}/*Packets.cs` | 6 / 16 / 25 / 15; pair each registration with its actual packet code |
| Server handlers | `TSOClient/FSO.Server/Servers/{City,Lot,Tasks}/*Server.cs::GetHandlers` | 21 / 5 / 2 classes; expand accepted methods via AriesPacketRouter, not class names alone |
| Data providers | Server `DataService/ServerDataService.cs`, client `Network/ClientDataService.cs` | Five per side: Avatar, Lot, City, Neighborhood, MayorRating |
| Regulators | `TSOClient/tso.client/Regulators/RegulatorsModule.cs` | Login, CityConnection, CreateASim, PurchaseLot, LotConnection |
| Database access | `TSOClient/FSO.Server.Database/DA/IDA.cs` and `SqlDA.cs` | 31 access properties |
| Database dispatch | `TSOClient/FSO.Server/Servers/City/Handlers/DBRequestWrapperHandler.cs` | Four handled request kinds; legacy enum size is not an active-handler count |
| Migrations | `TSOClient/FSO.Server.Database/DatabaseScripts/manifest.json` | 39 unique IDs; all referenced scripts present |
| Scheduled tasks | `TSOClient/FSO.Server/Servers/Tasks/TaskServer.cs::Engine.AddTask` | Six registered tasks; DbTaskType has eight values |
| Primitive editors | `TSOClient/FSO.IDE/EditorComponent/PrimitiveRegistry.cs::DescriptorById` | 42 explicit descriptors; unknown/subroutine fallback separate |
| IFF editors | `TSOClient/FSO.IDE/ResourceBrowser/IFFResComponent.cs::ChunkToControl` | Nine mappings; not the entire creator tool surface |

## Intake and upstream dispositions that must remain visible

- SDK defaults contribute 46 tracked candidate files: API Core 39, Server Core 2, Roslyn JIT 5. Evaluated conditions/imports/generated sources remain unresolved.
- Nineteen tracked csproj files lie outside the solution's 47 declarations. Each needs a disposition; presence does not establish active use or permission to omit it.
- Screen directory has 15 files, but FSO.Client.csproj compiles 13, including IGameScreen. The 12 concrete compiled screens differ from six views reached by nine GameController ChangeState calls. Patch/credits/sandbox/debug and logo reachability need separate accounting. LotScreen and LotScreenNew are not compiled there; AOT LinkEveryController is not an active screen registry.
- API Core's 23 controllers, 69 HTTP-verb annotations and 57 Route annotations are not 69 or 57 proven endpoints. Compose actual class/method routes and SDK discovery. Legacy Nancy's 11 controllers and 19 literal Get/Post registrations remain a separate runtime path.
- Mina gitlink `b2216c27fe6e17e4392130f778b6b54bf8c62bda` and MonoGame gitlink `74f7ef558111e62a7d0a2228b6f9714980e000fa` are uninitialized. Declared assimp-net has no tracked gitlink. Keep these intake gaps explicit.
- `multi_check` and `prune_abandoned_lots` are task enum values without AddTask registration. VMOnlineJobsCall offers jobs 1, 2, 4 and 5 and rejects 3. Original unavailability must not be presented as a missing newly promised feature or silently discarded.
- ObjectWindow, MainWindow, BHAVEditor and the BHAV/IFF managers include additional object, avatar, upgrade, thumbnail, live debugging, external IFF and undo/redo tools beyond the nine resource mappings.
- Missing installation content, patch/global/tuning closure and distribution rights remain release gates.

The next intake expansion must add exact member IDs, paths, hashes and reachability
dispositions for these cohorts. The initial 71 capability rows are preservation
anchors, not a total-game denominator and not a percent-complete estimate.
