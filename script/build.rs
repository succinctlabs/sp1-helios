#[allow(unused_imports)]
use sp1_build::{build_program_with_args, BuildArgs};

fn main() {
    build_program_with_args(
        "../program",
        BuildArgs {
            docker: true,
            tag: "v5.0.0".to_string(),
            output_directory: Some("../elf".to_string()),
            ..Default::default()
        },
    );
}
