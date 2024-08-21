# SP1 Helios

## Overview

On-chain Ethereum light client built with SP1.

- `/program`: The SP1 Helios program
- `/contracts`: Contracts for the Helios light client
- `/primitives`: Common types shared between the program, contract, and script
- `/script`: Scripts for getting the contract's genesis parameters and generating proofs

## Deployments

Holesky -> Sepolia Bridge: [`0x67952E466B2C61FE7B323262b289C8A90B264e27`](https://sepolia.etherscan.io/address/0x67952E466B2C61FE7B323262b289C8A90B264e27)

## Deploy a light client

### Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [SP1](https://docs.succinct.xyz/getting-started/install.html)

### 1. Generate genesis parameters

1. `cd ./script`
2. `cp .env.example .env`
3. By default, we'll use parameters for mainnet.
      - You may modify `SOURCE_CHAIN_ID` and `SOURCE_CONSENSUS_RPC_URL` inside `.env` with the values under [Supported Networks](#supported-networks).
      - For testing, the program defaults to mock proofs. If you want to generate proofs on the SP1 Cluster, set `SP1_PROVER` to `network`, `SP1_VERIFIER_ADDRESS` to a [deployed verifier found here]( https://docs.succinct.xyz/onchain-verification/contract-addresses.html), and `SP1_PRIVATE_KEY` to your whitelisted cluster private key.
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
   - `SOURCE_CONSENSUS_RPC_URL=https://www.lightclientdata.org`
- Sepolia Testnet
   - `SOURCE_CHAIN_ID=11155111`
   - `SOURCE_CONSENSUS_RPC_URL=http://unstable.sepolia.beacon-api.nimbus.team`
- Holesky Testnet
   - `SOURCE_CHAIN_ID=17000`
   - `SOURCE_CONSENSUS_RPC_URL=http://testing.holesky.beacon-api.nimbus.team`

**Destination (bridging to):**
- Helios supports bridging to any arbitrary evm chain.

## Generating hardcoded test cases
1. `cd ./script`
2.  Make sure you've set all .env variables inside script (for more context, follow the other steps)
3. `cargo run --release --bin gen-inputs`
   - Pass in a specific slot by appending ` -- --slot your_slot_number`
     
This will output a cbor-encoded file inside `script/examples`. You can load these bytes inside a test and pass it as input to the program. Feel free to modify the script further to accomadate for your test case.
