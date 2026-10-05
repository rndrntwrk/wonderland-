use std::io::{self, Write};
use swarm_a_replay::{hex, run_scenario, SCENARIOS, SEEDS};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let cases: Vec<(u32, u64)> = if args.is_empty() || args == ["--all"] {
        SCENARIOS
            .iter()
            .flat_map(|(scenario, _, _)| SEEDS.map(|seed| (*scenario, seed)))
            .collect()
    } else if args.len() == 4 && args[0] == "--scenario" && args[2] == "--seed" {
        vec![(
            args[1]
                .parse()
                .map_err(|_| "scenario must be a decimal u32")?,
            args[3].parse().map_err(|_| "seed must be a decimal u64")?,
        )]
    } else {
        return Err("Usage: sim-replay [--all | --scenario ID --seed DECIMAL_U64]".into());
    };
    let stdout = io::stdout();
    let mut writer = io::BufWriter::new(stdout.lock());
    for (scenario, seed) in cases {
        let report = run_scenario(scenario, seed)
            .map_err(|error| format!("scenario {scenario}, seed {seed}: {error}"))?;
        writeln!(writer, "{scenario} {seed} {}", hex(&report.to_bytes()))
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}
