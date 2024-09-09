use sp1_helper::BuildArgs;

fn main() {
    sp1_helper::build_program_with_args(
        &format!("{}/../program", env!("CARGO_MANIFEST_DIR")),
        BuildArgs {
            ignore_rust_version: true,
            ..Default::default()
        },
    );
}
