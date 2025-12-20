# Light Compressed Claim Reference Implementation

The program verifies claim eligibility via program_address derivation and decompresses tokens if valid.

- the claimant must be signer
- the unlock_slot must be >= slot
- the PDA must have previously received compressed-tokens, to be able to claim them.

## Note
This reference implementation is unaudited and should be tested before deployment. 
Use at your own risk.

If you have any questions, reach out on [Telegram](https://t.me/tilo_light) or Discord.
