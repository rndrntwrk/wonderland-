#[path = "support/probe.rs"]
mod probe;

fn main() {
    let result = std::env::args_os()
        .nth(1)
        .ok_or_else(|| "usage: source-replay INPUT_IFF".to_string())
        .and_then(|source| probe::run(std::path::Path::new(&source)));
    match result {
        Ok(value) => println!("{}", serde_json::to_string(&value).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
