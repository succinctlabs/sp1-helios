# SP1 Telepathy

## Deploy Contracts

### 1. Generate genesis parameters for contract

1. `cd ../script`
2. `RUST_LOG=info cargo run --release --bin genesis`

### 2. Deploy contracts with Foundry

1. Install [Foundry](https://book.getfoundry.sh/getting-started/installation)
2. `cd ../contracts`
3. `cp .env.example .env`
4. Paste the genesis parameters into `.env` and manually fill in the other parameters
5. `forge install`
6. `source .env`
7. `forge script script/Deploy.s.sol --rpc-url $RPC_URL --private-key $PRIVATE_KEY --etherscan-api-key $ETHERSCAN_API_KEY --broadcast --verify`
8. Take note of the light client contract address printed by the script
