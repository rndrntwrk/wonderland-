//! Authored process-test release. Production startup never constructs this.
#[allow(dead_code)]
#[path = "../../../tests/integration/swarm_b_runtime/support/cooked_fixture.rs"]
mod fixture;
fn main() {
    let root = std::env::args_os().nth(1).expect("fixture destination");
    let root = std::path::Path::new(&root);
    std::fs::create_dir(root).unwrap();
    let cooked = fixture::cook_custom(|file| {
        use wonderland_legacy_formats::{
            Limits,
            semantic::{decode_bhav, encode_bhav},
        };
        let chunk = file
            .chunks
            .iter_mut()
            .find(|c| c.key.kind == *b"BHAV" && c.key.id == 4097)
            .unwrap();
        let mut behavior = decode_bhav(&chunk.data, &Limits::default()).unwrap();
        behavior.instructions.truncate(1);
        behavior.instructions[0].opcode = 36;
        behavior.instructions[0].true_pointer = 254;
        behavior.instructions[0].false_pointer = 255;
        behavior.instructions[0].operand = [0; 8];
        chunk.data = encode_bhav(&behavior, &Limits::default()).unwrap();
    });
    cooked.write_release(root);
}
