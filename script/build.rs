use sp1_build::{build_program_with_args, BuildArgs};

fn main() {
    let args = BuildArgs {
        tag: "v3.0.0".to_string(),
        docker: true,
        elf_name: "sp1-helios-docker".to_string(),
        ..Default::default()
    };
    build_program_with_args("../program", args);
}
