# Components

An SP1 Helios implementation has a few key components:
- An `SP1Helios` contract. Contains the logic for verifying SP1 Helios proofs, storing the latest data from the Ethereum beacon chain, including the headers, execution state roots and sync committees.
- An `SP1Verifier` contract. Verifies arbitrary SP1 programs. Most chains will have canonical deployments
upon SP1's mainnet launch. Until then, users can deploy their own `SP1Verifier` contracts to verify
SP1 programs on their chain. The SP1 Helios implementation will use the `SP1Verifier` contract to verify
the proofs of the SP1 Helios program.
- The SP1 Helios program. An SP1 program that verifies the consensus of a source chain in the execution environment of a destination chain using the `helios` library.
- The operator. A Rust script that fetches the latest data from a deployed `SP1Helios` contract and an Ethereum beacon chain, determines the block to request, requests for/generates a proof, and relays the proof to the `SP1Helios` contract.
