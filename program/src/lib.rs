#![deny(missing_docs)]

//! A program that takes an "AATA (associated airdrop token account)", verifies
//! that a signer is eligible to claim the account, and decompresses it.
//!
//! An "AATA" is a compressed token account that is owned by this program and is
//! derived from the respective mint, claimant, and unlock slot.
mod entrypoint;
mod error;
/// Processes the "claim_and_decompress" instruction. No Mapping used.
pub mod processor;

pub use solana_program;

solana_program::declare_id!("7UHB3CfWv7SugNhfdyP7aeZJPMjnpd9zJ7xYkHozB3Na");

// TODO: add Instruction serde unit test.
/// Build a claim_and_decompress instruction
///
// pub fn build_claim_and_decompress(slot: u64, signer_pubkeys: &[&Pubkey]) -> Instruction {
//     Instruction {
//         program_id: id(),
//         accounts: signer_pubkeys
//             .iter()
//             .map(|&pubkey| AccountMeta::new_readonly(*pubkey, true))
//             .collect(),
//         data: data,
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
}
