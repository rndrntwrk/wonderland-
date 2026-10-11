fn main() {
    for mode in 0..3 {
        for avatars in [32, 64] {
            for tick in [0, 15, 30] {
                match wonderland_presentation_replay::observe(mode, avatars, tick) {
                    Ok(record) => println!("{record}"),
                    Err(error) => {
                        eprintln!("{error}");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}
