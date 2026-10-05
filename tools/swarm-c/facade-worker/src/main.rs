#![forbid(unsafe_code)]
use std::path::Path;
use wonderland_facade_worker::{
    read_request, render_to_directory, synthetic_fixture, WorkerResult,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("facade-worker: {error}");
        std::process::exit(1);
    }
}
fn run() -> WorkerResult<()> {
    let args: Vec<_> = std::env::args_os().collect();
    let (input, output) = match args.get(1).and_then(|s| s.to_str()) {
        Some("fixture") if args.len() == 3 => (synthetic_fixture(), Path::new(&args[2])),
        Some("render") if args.len() == 4 => (read_request(Path::new(&args[2]))?, Path::new(&args[3])),
        _ => return Err("usage: facade-worker fixture NEW_OUTPUT_DIRECTORY | facade-worker render REQUEST.wlcdr NEW_OUTPUT_DIRECTORY".into()),
    };
    let summary = render_to_directory(input, output)?;
    println!("{{\"key\":\"{}\",\"artifact_sha256\":\"{}\",\"images\":{},\"reservation_bytes\":{},\"work_units\":{}}}", summary.key, summary.digest, summary.image_count, summary.reservation_bytes, summary.work_units);
    Ok(())
}
