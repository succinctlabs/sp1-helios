# SP1 Helios

## Overview

On-chain Ethereum light client built with SP1.

- `/program`: The SP1 Helios program
- `/contracts`: Contracts for the Helios light client
- `/primitives`: Common types shared between the program, contract, and script
- `/script`: Scripts for getting the contract's genesis parameters and generating proofs

## Deployments

Holesky -> Sepolia Bridge: [`0x53544ba8e5504Df8569E1F2fEd8b39af9e7F5B71`](https://sepolia.etherscan.io/address/0x53544ba8e5504Df8569E1F2fEd8b39af9e7F5B71)

## Deploy a light client

### Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [SP1](https://docs.succinct.xyz/getting-started/install.html)

### 1. Generate genesis parameters
  1. `cd ./script`
  2. `cp .env.example .env`
  3. Modify .env if needed.
      - **Source chain:**
          - **Mainnet by default**     
          - `SOURCE_CHAIN_ID`
          - `SOURCE_CONSENSUS_RPC_URL`
          - Use values under [Supported Networks](#supported-networks).
      - **Proofs:**
          - **Mock by default** for testing
          - Generate real proofs on cluster (for prod):
              - `SP1_PROVER`: `network`
              - `SP1_VERIFIER_ADDRESS`: [use a deployed verifier found here]( https://docs.succinct.xyz/onchain-verification/contract-addresses.html)
              - `SP1_PRIVATE_KEY`: your whitelisted cluster private key.
      - The other .env values will be filled out at a later step.
  7. `RUST_LOG=info cargo run --release --bin genesis`

### 2. Deploy contracts

1. `cd ../contracts`
2. `cp .env.example .env`
3. Paste the genesis parameters into `.env` and manually fill in the other parameters
4. `forge install`
5. `source .env`
6. `forge script script/Deploy.s.sol --rpc-url $RPC_URL --private-key $PRIVATE_KEY --etherscan-api-key $ETHERSCAN_API_KEY --broadcast --verify --via-ir`
7. Take note of the light client contract address printed by the script

   ![alt text](./return-image.png)

### 3. Run light client

Continuously generate proofs & keep light client updated with chain

1. `cd ../script`
2. Paste in the contract address in `.env` and fill out the rest of the parameters.
3. `RUST_LOG=info cargo run --release --bin operator`

## Supported Networks
Public light client RPCs are hard to come by - for convenience, here are some example values [courtesy of Nimbus](https://github.com/status-im/nimbus-eth2?tab=readme-ov-file#quickly-test-your-tooling-against-nimbus) (as of Aug 20, 2024)

**Source (bridging from):**
- Mainnet
   - `SOURCE_CHAIN_ID=1`
   - `SOURCE_CONSENSUS_RPC_URL=http://unstable.mainnet.beacon-api.nimbus.team/`
- Sepolia Testnet
   - `SOURCE_CHAIN_ID=11155111`
   - `SOURCE_CONSENSUS_RPC_URL=http://unstable.sepolia.beacon-api.nimbus.team`
- Holesky Testnet
   - `SOURCE_CHAIN_ID=17000`
   - `SOURCE_CONSENSUS_RPC_URL=http://testing.holesky.beacon-api.nimbus.team`

**Destination (bridging to):**
- Helios supports bridging to any arbitrary EVM chain.

## Testing `sp1-helios`
Once you've configured your environment, you can test that the light client program can update to a new consensus state by running:

```bash
RUST_LOG=infocargo run --bin test --release
```

or to test a specific slot:

```bash
RUST_LOG=infocargo run --bin test --release -- --slot your_slot_number
```

This will fetch the relevant data from the source chain and execute the program with the generated inputs. If this runs successfully, the program can update to a new consensus state.
