#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(wonderland_web_shell::app::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("Wonderland's browser shell runs on wasm32-unknown-unknown. Use trunk serve.");
}
