//! TEST ONLY: synthetic standalone-format resources, never a production avatar pack.
#[allow(dead_code)]
#[path = "../tests/support/native_avatar_bank.rs"]
mod bank;
#[path = "../tests/support/native_camera_witness.rs"]
mod camera;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = std::env::args_os()
        .nth(1)
        .ok_or("Supply an output directory for synthetic fixtures")?;
    let destination = std::path::PathBuf::from(destination);
    std::fs::create_dir_all(&destination)?;
    for (name, bytes) in camera::volumetric(bank::files()) {
        let file = destination.join(name);
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(file)?;
        std::io::Write::write_all(&mut out, &bytes)?;
    }
    println!(
        "base.anim frames {}",
        bank::bank().animation("base.anim")?.source().num_frames
    );
    Ok(())
}
