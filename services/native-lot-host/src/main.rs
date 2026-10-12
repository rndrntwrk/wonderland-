//! Private supervisor pipe only. No HTTP listener or browser-authentication bypass.
mod bounded;
mod events;
mod loader;
mod runtime;
use serde_json::json;
use std::io::{self, BufRead, Write};
use wonderland_native_host_contracts::RequestV1;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 2 {
        return Err("usage: wonderland-native-lot-host CONFIG.json".into());
    }
    let (state, config, binding, report) = loader::load(std::path::Path::new(&args[1]))?;
    let mut host = runtime::Host::new(state, config, binding, report)?;
    let mut out = io::BufWriter::new(io::stdout().lock());
    serde_json::to_writer(&mut out, &host.handle(RequestV1::Inspect)?)?;
    writeln!(out)?;
    out.flush()?;
    let mut input = io::stdin().lock();
    loop {
        let mut line = Vec::new();
        // Bounded streaming read: never allocate an unbounded JSON line.
        loop {
            let buf = input.fill_buf()?;
            if buf.is_empty() {
                break;
            }
            let n = buf
                .iter()
                .position(|b| *b == b'\n')
                .map_or(buf.len(), |n| n + 1);
            if line.len() + n > 1024 * 1024 {
                return Err("supervisor input exceeds 1 MiB".into());
            }
            line.extend_from_slice(&buf[..n]);
            input.consume(n);
            if line.last() == Some(&b'\n') {
                break;
            }
        }
        if line.is_empty() {
            break;
        }
        let reply = serde_json::from_slice::<RequestV1>(&line)
            .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })
            .and_then(|r| host.handle(r));
        let reply = match reply {
            Ok(value) => value,
            Err(error) => json!({"kind":"denied","reason":error.to_string()}),
        };
        serde_json::to_writer(&mut out, &reply)?;
        writeln!(out)?;
        out.flush()?;
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("native lot host stopped: {error}");
        std::process::exit(1);
    }
}
