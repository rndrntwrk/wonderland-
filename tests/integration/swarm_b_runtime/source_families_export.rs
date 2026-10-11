#[path = "support/families.rs"]
mod families;

fn main() {
    let result = std::env::args_os()
        .nth(1)
        .ok_or_else(|| "usage: source-families REPOSITORY_ROOT".to_owned())
        .and_then(|root| families::run(std::path::Path::new(&root)))
        .and_then(|value| serde_json::to_string(&value).map_err(|error| error.to_string()));
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
