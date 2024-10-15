# SP1 Helios

## Overview

On-chain Ethereum light client built with SP1.

- `/program`: The SP1 Helios program
- `/contracts`: Contracts for the Helios light client
- `/primitives`: Common types shared between the program, contract, and script
- `/script`: Scripts for getting the contract's genesis parameters and generating proofs

## Deploy an SP1 Helios Light Client

### Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [SP1](https://docs.succinct.xyz/getting-started/install.html)

### 1. Generate genesis parameters
  1. `cp .env.example .env`
  2. Modify .env if needed.
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
  3. `RUST_LOG=info cargo run --release --bin genesis`

### 2. Deploy SP1 Helios Contract

To deploy the SP1 Helios contract, you need to fill out the following variables in your 

1. `cd ../contracts`
2. `cp .env.example .env`
3. Paste the genesis parameters into `.env` and manually fill in the other parameters
4. `forge install`
5. `source .env`
6. `forge script script/Deploy.s.sol --ffi --rpc-url $RPC_URL --private-key $PRIVATE_KEY --etherscan-api-key $ETHERSCAN_API_KEY --broadcast --verify`
7. Take note of the light client contract address printed by the script

   ![alt text](./return-image.png)

### 3. Run light client

Continuously generate proofs & keep light client updated with chain

1. Paste in the contract address in `.env` and fill out the rest of the parameters.
2. `RUST_LOG=info cargo run --release --bin operator`

## Supported Networks
To run `sp1-helios` we recommend getting a Beacon Chain node from Quicknode, or one of the providers from this list of [L1 Ethereum beacon chain RPC providers](https://github.com/a16z/helios/blob/master/README.md#configuration-files-).

**Source (bridging from):**
- Mainnet
   - `SOURCE_CHAIN_ID=1`
   - `SOURCE_CONSENSUS_RPC_URL=<ETHEREUM_BEACON_CHAIN_RPC_URL>`
- Sepolia Testnet
   - `SOURCE_CHAIN_ID=11155111`
   - `SOURCE_CONSENSUS_RPC_URL=<SEPOLIA_BEACON_CHAIN_RPC_URL>`
- Holesky Testnet
   - `SOURCE_CHAIN_ID=17000`
   - `SOURCE_CONSENSUS_RPC_URL=<HOLESKY_BEACON_CHAIN_RPC_URL>`

**Destination (bridging to):**
- Helios supports bridging to any arbitrary EVM chain.

**Warning:** Sepolia and Holesky networks are currently not functioning as expected. Use with caution.

## Testing `sp1-helios`
Once you've configured your environment, you can test that the light client program can update to a new consensus state by running:
