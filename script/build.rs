#[allow(unused_imports)]
use sp1_build::{build_program_with_args, BuildArgs};

fn main() {
    build_program_with_args(
        "../program",
        BuildArgs {
            docker: true,
            elf_name: Some("sp1-helios-elf".to_string()),
            tag: "v4.1.3".to_string(),
            output_directory: Some("../elf".to_string()),
            ..Default::default()
        },
    );
}
