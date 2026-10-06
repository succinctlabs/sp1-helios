# Reproducible Builds

## Overview

Verify the program ELFs before you use them to generate proofs.
Run these commands from the repository root of the release you want to verify.

## Prerequisites

Install the [SP1 toolchain](https://docs.succinct.xyz/getting-started/install.html#option-1-prebuilt-binaries-recommended).

Use the SP1 version that built the checked-in ELFs:

```bash
sp1up --version 6.8.1
```

Check the installed version:

```bash
cargo prove --version
```

## Rebuild the program ELFs

Check that Docker is running:

```bash
docker ps
```

Build all three programs with the pinned Docker image and lockfile:

```bash
cd program
cargo prove build --docker --tag v6.8.1 --locked --output-directory ../elf
cd ..
git diff --exit-code -- elf/
```

The diff check succeeds when the rebuilt ELFs match the checked-in files.

## Verify the program keys

Derive the verification keys from the rebuilt ELFs:

```bash
SP1_SKIP_PROGRAM_BUILD=true cargo run --locked --release -p sp1-helios-script --bin vkey
```

Compare each key with the corresponding key configured in the SP1 Helios smart contract.
See the [deployment guide](deployment.md) for program key updates.
