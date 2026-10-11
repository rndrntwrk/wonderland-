#[path = "../../../tests/integration/swarm_b_runtime/support/probe.rs"]
mod probe;
mod release_cli;

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.first().and_then(|arg| arg.to_str()) {
        Some("release-prepare" | "release-plan" | "release-load" | "release-replay") => {
            use std::io::Write;
            let report = release_cli::run(&args)?;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&report).and_then(|_| stdout.write_all(b"\n")).map_err(|_| "report output failed")?;
        }
        Some("source-replay") if args.len() == 2 => {
            let value = probe::run(std::path::Path::new(&args[1]))?;
            println!("{}", serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?);
        }
        Some("capabilities") if args.len() == 1 => println!("{}", serde_json::json!({
            "sim_revision": wonderland_content_runtime_bridge::SIM_CORE_REVISION,
            "content_conversion": true,
            "verified_cooked_release": true,
            "cooked_replica_query_and_tick_harness": true,
            "isolated_behavior_query": true,
            "typed_snapshot_watches": true,
            "isolated_accepted_tick_replay": true,
            "frame_positions": "stored positions only",
            "instruction_step": false,
            "executed_instruction_trace": false,
            "live_queue_adapter": false,
            "full_check_tree_provider": false,
            "source_replay_scope": "one unmodified BHAV in a declared authored state harness"
        })),
        None | Some("--help" | "-h") => println!("runtime-bridge capabilities\nruntime-bridge source-replay INPUT_IFF\n{}\n\nThe source replay command requires the pinned Casino_2-Tile_Bar_CC.iff.\nGeneral content conversion, snapshot watches and whole-tick replay are library APIs.\nNo breakpoint, instruction-step or executed-trace provider is exposed.", release_cli::HELP),
        _ => return Err("unknown command; run runtime-bridge --help".into()),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
