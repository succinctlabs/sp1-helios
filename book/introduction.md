# SP1 Vector

## Overview

Implementation of zero-knowledge proof circuits for [Vector](https://blog.availproject.org/data-attestation-bridge/), Avail's Data Attestation Bridge in SP1.

- `/program`: The SP1 Vector program.
- `/primitives`: Libraries for types and helper functions used in the program.
- `/script`: Scripts for getting the contract's genesis parameters and deploying the operator to 
    update the light client.
- `/services`: RPC fetcher for the `script` + the justification indexer.
- `/contracts`: The contract's source code and deployment scripts. 
- `/query`: Contains the logic for querying data root proofs from the contracts. Automatically deploys to https://vectorx-query.succinct.xyz.
