//! Fresh baseline registries from VMContext.InitVMConfig, not the legacy process-global stale slots.
use crate::vm::VmMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageStatus {
    Implemented,
    SourceNoOp,
    Partial,
    HostAdapter,
    RequestOnly,
    Unsupported,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimitiveInfo {
    pub opcode: u16,
    pub handler: &'static str,
    pub operand: &'static str,
    pub status: CoverageStatus,
    pub source_line: u16,
}
macro_rules! entry {
    ($op:literal,$handler:literal,$operand:literal,$status:ident,$line:literal) => {
        PrimitiveInfo {
            opcode: $op,
            handler: $handler,
            operand: $operand,
            status: CoverageStatus::$status,
            source_line: $line,
        }
    };
}

/// 48 registrations shared by TSO and TS1; mode overrides are applied by `primitive_info`.
pub const COMMON_PRIMITIVES: [PrimitiveInfo; 48] = [
    entry!(0, "VMSleep", "VMSleepOperand", Implemented, 94),
    entry!(2, "VMExpression", "VMExpressionOperand", Implemented, 103),
    entry!(4, "VMGrab", "VMGrabOperand", HostAdapter, 112),
    entry!(5, "VMDrop", "VMDropOperand", HostAdapter, 119),
    entry!(
        6,
        "VMChangeSuitOrAccessory",
        "VMChangeSuitOrAccessoryOperand",
        HostAdapter,
        126
    ),
    entry!(7, "VMRefresh", "VMRefreshOperand", HostAdapter, 133),
    entry!(
        8,
        "VMRandomNumber",
        "VMRandomNumberOperand",
        Implemented,
        140
    ),
    entry!(9, "VMBurn", "VMBurnOperand", HostAdapter, 147),
    entry!(
        11,
        "VMGetDistanceTo",
        "VMGetDistanceToOperand",
        Implemented,
        156
    ),
    entry!(
        12,
        "VMGetDirectionTo",
        "VMGetDirectionToOperand",
        Implemented,
        163
    ),
    entry!(
        13,
        "VMPushInteraction",
        "VMPushInteractionOperand",
        HostAdapter,
        170
    ),
    entry!(
        14,
        "VMFindBestObjectForFunction",
        "VMFindBestObjectForFunctionOperand",
        HostAdapter,
        177
    ),
    entry!(15, "VMBreakPoint", "VMBreakPointOperand", SourceNoOp, 184),
    entry!(
        16,
        "VMFindLocationFor",
        "VMFindLocationForOperand",
        HostAdapter,
        191
    ),
    entry!(
        17,
        "VMIdleForInput",
        "VMIdleForInputOperand",
        HostAdapter,
        198
    ),
    entry!(
        18,
        "VMRemoveObjectInstance",
        "VMRemoveObjectInstanceOperand",
        HostAdapter,
        205
    ),
    entry!(
        20,
        "VMRunFunctionalTree",
        "VMRunFunctionalTreeOperand",
        HostAdapter,
        214
    ),
    entry!(21, "VMShowString", "VMShowStringOperand", HostAdapter, 223),
    entry!(22, "VMLookTowards", "VMLookTowardsOperand", Partial, 230),
    entry!(23, "VMPlaySound", "VMPlaySoundOperand", RequestOnly, 237),
    entry!(
        24,
        "VMRelationship",
        "VMOldRelationshipOperand",
        HostAdapter,
        244
    ),
    entry!(
        26,
        "VMRelationship",
        "VMRelationshipOperand",
        HostAdapter,
        252
    ),
    entry!(
        27,
        "VMGotoRelativePosition",
        "VMGotoRelativePositionOperand",
        Partial,
        259
    ),
    entry!(
        28,
        "VMRunTreeByName",
        "VMRunTreeByNameOperand",
        HostAdapter,
        266
    ),
    entry!(
        29,
        "VMSetMotiveChange",
        "VMSetMotiveChangeOperand",
        HostAdapter,
        273
    ),
    entry!(30, "VMSysLog", "VMSysLogOperand", SourceNoOp, 281),
    entry!(31, "VMSetToNext", "VMSetToNextOperand", Partial, 288),
    entry!(
        32,
        "VMTestObjectType",
        "VMTestObjectTypeOperand",
        Implemented,
        295
    ),
    entry!(
        35,
        "VMSpecialEffect",
        "VMSpecialEffectOperand",
        RequestOnly,
        306
    ),
    entry!(
        36,
        "VMDialogPrivateStrings",
        "VMDialogOperand",
        RequestOnly,
        313
    ),
    entry!(
        37,
        "VMTestSimInteractingWith",
        "VMTestSimInteractingWithOperand",
        HostAdapter,
        320
    ),
    entry!(
        38,
        "VMDialogGlobalStrings",
        "VMDialogOperand",
        RequestOnly,
        327
    ),
    entry!(
        39,
        "VMDialogSemiGlobalStrings",
        "VMDialogOperand",
        RequestOnly,
        334
    ),
    entry!(
        40,
        "VMOnlineJobsCall",
        "VMOnlineJobsCallOperand",
        RequestOnly,
        341
    ),
    entry!(
        41,
        "VMSetBalloonHeadline",
        "VMSetBalloonHeadlineOperand",
        HostAdapter,
        348
    ),
    entry!(
        42,
        "VMCreateObjectInstance",
        "VMCreateObjectInstanceOperand",
        Partial,
        355
    ),
    entry!(43, "VMDropOnto", "VMDropOntoOperand", HostAdapter, 362),
    entry!(44, "VMAnimateSim", "VMAnimateSimOperand", HostAdapter, 369),
    entry!(
        45,
        "VMGotoRoutingSlot",
        "VMGotoRoutingSlotOperand",
        Partial,
        376
    ),
    entry!(46, "VMSnap", "VMSnapOperand", Partial, 383),
    entry!(47, "VMReach", "VMReachOperand", Partial, 390),
    entry!(
        48,
        "VMStopAllSounds",
        "VMStopAllSoundsOperand",
        RequestOnly,
        397
    ),
    // Source really registers the animate operand for notify; its handler does not inspect it.
    entry!(
        49,
        "VMNotifyOutOfIdle",
        "VMAnimateSimOperand",
        HostAdapter,
        404
    ),
    entry!(
        50,
        "VMChangeActionString",
        "VMChangeActionStringOperand",
        HostAdapter,
        411
    ),
    entry!(
        62,
        "VMInvokePlugin",
        "VMInvokePluginOperand",
        RequestOnly,
        422
    ),
    entry!(
        63,
        "VMGetTerrainInfo",
        "VMGetTerrainInfoOperand",
        Partial,
        429
    ),
    entry!(
        65,
        "VMFindBestAction",
        "VMFindBestActionOperand",
        HostAdapter,
        438
    ),
    entry!(
        67,
        "VMInventoryOperations",
        "VMInventoryOperationsOperand",
        RequestOnly,
        448
    ),
];
pub fn primitive_info(mode: VmMode, opcode: u16) -> Option<PrimitiveInfo> {
    if mode == VmMode::Ts1 {
        match opcode {
            1 => {
                return Some(entry!(
                    1,
                    "VMGenericTS1Call",
                    "VMGenericTS1CallOperand",
                    RequestOnly,
                    464
                ))
            }
            3 => {
                return Some(entry!(
                    3,
                    "VMFindBestAction",
                    "VMFindBestActionOperand",
                    HostAdapter,
                    457
                ))
            }
            19 => {
                return Some(entry!(
                    19,
                    "VMTS1MakeNewCharacter",
                    "VMTS1MakeNewCharacterOperand",
                    Unsupported,
                    471
                ))
            }
            25 => {
                return Some(entry!(
                    25,
                    "VMTS1Budget",
                    "VMTransferFundsOperand",
                    HostAdapter,
                    478
                ))
            }
            30 => {
                return Some(entry!(
                    30,
                    "VMGosubFoundAction",
                    "VMGosubFoundActionOperand",
                    HostAdapter,
                    485
                ))
            }
            51 => {
                return Some(entry!(
                    51,
                    "VMTS1InventoryOperations",
                    "VMTS1InventoryOperationsOperand",
                    HostAdapter,
                    492
                ))
            }
            _ => {}
        }
    } else {
        match opcode {
            1 => {
                return Some(entry!(
                    1,
                    "VMGenericTSOCall",
                    "VMGenericTSOCallOperand",
                    RequestOnly,
                    502
                ))
            }
            25 => {
                return Some(entry!(
                    25,
                    "VMTransferFunds",
                    "VMTransferFundsOperand",
                    Partial,
                    509
                ))
            }
            _ => {}
        }
    }
    COMMON_PRIMITIVES
        .iter()
        .find(|entry| entry.opcode == opcode)
        .copied()
}
pub fn registry(mode: VmMode) -> Vec<PrimitiveInfo> {
    (0..256)
        .filter_map(|opcode| primitive_info(mode, opcode))
        .collect()
}
