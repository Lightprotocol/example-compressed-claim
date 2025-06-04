# Light Compressed Claim Reference Implementation

The program verifies claim eligibility via program_address derivation and decompresses tokens if valid.

- the claimant must be signer
- the unlock_slot must be >= slot
- the PDA must have previously received compressed-tokens, to be able to claim them.

## Note

This code is unaudited. Do not use in production.

If you have any questions, reach out on [Telegram](https://t.me/swen_light) or [Discord](https://discord.gg/lightprotocol).
