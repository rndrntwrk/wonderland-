//! Synthetic source DTO fixture. Not an original game save.
use wonderland_vm_protocol::snapshot::*;

pub fn fixture() -> Snapshot {
    let wall = Wall {
        segments: 0,
        patterns: [0; 4],
        styles: [0; 2],
    };
    let mut walls = vec![vec![wall; 9]; 2];
    walls[0][4] = Wall {
        segments: 1 | 2 | 4 | 8,
        patterns: [9, 10, 11, 12],
        styles: [1, 2],
    };
    walls[1][4] = Wall {
        segments: 32,
        patterns: [13, 14, 15, 16],
        styles: [17, 18],
    };
    let mut floors = vec![vec![0; 9]; 2];
    floors[0][4] = 9;
    floors[1][4] = 10;
    let entity = Entity {
        object_id: 7,
        persist_id: 4_000_000_001,
        platform: EntityPlatform::Object {
            budget: 0,
            owner_id: 42,
            wear: 0,
            repair_quarters: 0,
            flags: 0,
            upgrade_level: 0,
        },
        object_data: vec![],
        my_list: vec![],
        headline: None,
        guid: 0xFEDCBA98,
        master_guid: 0,
        main_param: 0,
        main_stack_object: 0,
        contained: vec![],
        container: 0,
        container_slot: 0,
        attributes: vec![],
        object_relationships: vec![],
        persistent_relationships: vec![],
        dynamic_flags: u64::MAX,
        dynamic_flags2: 9_007_199_254_740_993,
        position: Position {
            x: 24,
            y: 24,
            level: 2,
        },
        timestamp_lockout: 0,
        light_color: u32::MAX,
        appearance: Appearance::Object {
            direction: 0x04,
            disabled: 0,
        },
    };
    Snapshot {
        version: 38,
        compressed: false,
        semantics: RestoreSemantics::RefreshOnly,
        context: Context {
            clock: Clock {
                ticks: 0,
                minute_fractions: 0,
                ticks_per_minute: 30,
                minutes: 0,
                hours: 12,
                day: 1,
                month: 1,
                year: 2000,
                fire_percent: 0,
                utc_start: 0,
            },
            architecture: Architecture {
                width: 3,
                height: 3,
                stories: 2,
                terrain_light: 0,
                terrain_dark: 1,
                heights: vec![0, 0, 0, 0, 16, 32, 0, 48, 64],
                grass: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
                walls,
                floors,
                walls_dirty: false,
                floors_dirty: false,
                roof_style: 16,
                roof_pitch: 0.66,
                id_map: None,
                fine_buildable: None,
                build_buy_enabled: true,
            },
            ambience_bits: 0,
            random_seed: 0,
        },
        entities: vec![entity],
        threads: vec![],
        multitile_groups: vec![],
        global_state: vec![],
        platform: LotState {
            name: "Snapshot fixture".into(),
            lot_id: 0x012301AB,
            surrounding_blends: vec![[0; 4]; 9],
            surrounding_roads: [0; 9],
            surrounding_heights: [0; 16],
            category: 7,
            size: 0,
            owner_id: 42,
            roommates: vec![],
            build_roommates: vec![],
            job_ui: None,
            skill_mode: 0,
            chat_channels: vec![],
            neighborhood_id: 9,
        },
        next_object_id: 8,
        tuning: None,
        consumed: 3,
        source_body: vec![1, 2, 3],
    }
}
