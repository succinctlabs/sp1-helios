# SP1 Helios

## Overview

Implementation of [Helios](https://github.com/a16z/helios) in Rust for SP1.

- `/program`: The SP1 Helios program.
- `/primitives`: Libraries for types and helper functions used in the program.
- `/script`: Scripts for getting the contract's genesis parameters and generating proofs

## Generate a proof
1. `cd ./script`
2. `RUST_LOG=info cargo run --release`
