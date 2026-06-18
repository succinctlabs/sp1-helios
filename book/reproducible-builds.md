# Reproducible Builds

The two guest ELFs in `elf/` (`light_client` and `storage`) determine
the vkeys stored in `SP1Helios`. For a deployment to be auditable, those
ELFs must be reproducible from source.

## Prerequisites

The [`cargo prove`](https://docs.succinct.xyz/docs/sp1/getting-started/install)
toolchain. Install or update with:

```bash
curl -L https://sp1.succinct.xyz | bash
sp1up
cargo prove --version
```

Reproducible builds happen inside a Docker image, so a working Docker
daemon is required:

```bash
docker ps
```

## Build

Build both guests with the SP1 toolchain version pinned to the one
declared in the workspace `Cargo.toml` (`sp1-sdk` / `sp1-build`). Run
from the workspace root:

```bash
cd program
cargo prove build --docker --tag v<SP1_VERSION> --output-directory ../elf
```

Replace `<SP1_VERSION>` with the version in `Cargo.toml` (for example,
`v5.2.4`). Both `light_client` and `storage` binaries will be written
to `../elf/`. The output should be byte-identical across machines.

## Verify the vkeys

The `vkey` script reads each ELF and prints its verification key:

```bash
cargo run --bin vkey --release
```

The printed `Light Client Verifying Key` must equal `SP1Helios.lightClientVkey()`
on the deployed contract, and `Storage Verifying Key` must equal
`SP1Helios.storageSlotVkey()`. If they do not match, the deployment is
either using a different guest version or a non-reproducible build — do
not trust it without resolving the discrepancy (e.g. by asking the
guardian to rotate the vkeys via `updateLightClientVkey` /
`updateStorageSlotVkey`).
