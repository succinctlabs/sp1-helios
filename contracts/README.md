# TelepathyX Contracts

## Deploy TelepathyX Contract

Fill out `.env` following `.env.example`.

Run the following commands to deploy the TelepathyX contract for your specific network in `/contracts`:

```
forge script script/Deploy.s.sol --rpc-url $RPC_URL --private-key $PRIVATE_KEY --etherscan-api-key $ETHERSCAN_API_KEY --broadcast --verify
```
