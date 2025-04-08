# Reproducible Builds

## Overview

When deploying SP1 Helios in production, it's important to ensure that the program used when generating proofs is reproducible.

## Prerequisites

You first need to install the [cargo prove](https://docs.succinct.xyz/getting-started/install.html#option-1-prebuilt-binaries-recommended) toolchain.

Ensure that you have the latest version of the toolchain by running:

```bash
sp1up
```

Confirm that you have the toolchain installed by running:

```bash
cargo prove --version
```

## Verify the SP1 Helios binary

To build the SP1 Helios binary, first ensure that Docker is running.

```bash
docker ps
```

Then build the binaries:

```bash
cd program

# Builds the SP1 Helios binary using the corresponding Docker tag, output directory and ELF name.
cargo prove build --docker --tag v4.1.7 --elf-name sp1-helios-elf --output-directory ../elf
```

Now, verify the binaries by confirming the output of `vkey` matches the vkeys on the contract. The `vkey` program outputs the verification key
based on the ELF in `/elf`.

```bash
cargo run --bin vkey --release
```
