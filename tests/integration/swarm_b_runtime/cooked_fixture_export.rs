#[path = "support/cooked_fixture.rs"]
mod fixture;

fn main() {
    let destination = std::env::args_os()
        .nth(1)
        .expect("expected a new fixture directory");
    let destination = std::path::Path::new(&destination);
    std::fs::create_dir(destination).unwrap();
    fixture::cook().write_release(destination);
    println!("authored cooked fixture written; all original source files removed");
}
