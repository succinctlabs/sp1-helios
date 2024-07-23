# SP1 Telepathy

## Overview

On-chain Ethereum light client built with SP1.

- `/program`: The SP1 Telepathy program
- `/contracts`: Contracts for the Telepathy light client
- `/primitives`: Common types shared between the program, contract, and script
- `/script`: Scripts for getting the contract's genesis parameters and generating proofs


## Deployments
Sepolia: [`0x91F06297221f09824A0052c24BE4536b93d3e2b9`](https://sepolia.etherscan.io/address/0x91F06297221f09824A0052c24BE4536b93d3e2b9)

## Deploy a light client

### Prerequisites 
- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [SP1](https://docs.succinct.xyz/getting-started/install.html)
  
### 1. Generate genesis parameters

1. `cd ./script`
2. `RUST_LOG=info cargo run --release --bin genesis`
   
For testing, the contract defaults to verifying mock proofs. If you want to verify real proofs, pass in the address of the verifier as an argument:

e.g `RUST_LOG=info cargo run --release --bin genesis -- --verifier 0x3B6041173B80E77f038f3F2C0f9744f04837185e`

You can find a list of [deployed verifiers here.](https://github.com/succinctlabs/sp1/blob/main/book/onchain-verification/contract-addresses.md)

### 2. Deploy contracts

1. `cd ../contracts`
2. `cp .env.example .env`
3. Paste the genesis parameters into `.env` and manually fill in the other parameters
4. `forge install`
5. `source .env`
6. `forge script script/Deploy.s.sol --rpc-url $RPC_URL --private-key $PRIVATE_KEY --etherscan-api-key $ETHERSCAN_API_KEY --broadcast --verify`
7. Take note of the light client contract address printed by the script
   
   ![alt text](./return-image.png)

### 3. Run light client
Continuously generate proofs & keep light client updated with chain
1. `cd ../script`
2. `cp .env.example .env`
3. Paste in the contract address in `.env` and fill out the other parameters.
4. `RUST_LOG=info cargo run --release --bin operator`
