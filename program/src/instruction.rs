use borsh::{BorshDeserialize, BorshSerialize};
use light_compressed_account::compressed_account::PackedMerkleContext;
use light_compressed_account::instruction_data::compressed_proof::CompressedProof;
use solana_program::pubkey::Pubkey;

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub enum ClaimProgramInstruction {
    ClaimAndDecompress {
        proof: Option<CompressedProof>,
        root_index: u16,
        merkle_context: PackedMerkleContext,
        amount: u64,
        lamports: Option<u64>,
        mint: Pubkey,
        unlock_slot: u64,
        bump_seed: u8,
    },
}

#[cfg(not(target_os = "solana"))]
pub mod client {
    use super::*;
    use solana_program::instruction::{AccountMeta, Instruction};
    #[derive(Debug)]
    pub struct ClaimAccounts {
        pub claimant: Pubkey,
        pub fee_payer: Pubkey,
        pub associated_airdrop_pda: Pubkey,
        pub ctoken_cpi_authority_pda: Pubkey,
        pub light_system_program: Pubkey,
        pub registered_program_pda: Pubkey,
        pub noop_program: Pubkey,
        pub account_compression_authority: Pubkey,
        pub account_compression_program: Pubkey,
        pub ctoken_program: Pubkey,
        pub token_pool_pda: Pubkey,
        pub decompress_destination: Pubkey,
        pub token_program: Pubkey,
        pub system_program: Pubkey,
        pub state_tree: Pubkey,
        pub queue: Pubkey,
    }

    /// Build a claim instruction in the client.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[signer]` Claimant
    ///   1. `[signer]` Fee payer
    ///   2. `[]` Associated airdrop PDA
    ///   3. `[]` CToken CPI authority PDA
    ///   4. `[]` Light system program
    ///   5. `[]` Registered program PDA
    ///   6. `[]` Noop program
    ///   7. `[]` Account compression authority
    ///   8. `[]` Account compression program
    ///   9. `[]` CToken program
    ///  10. `[]` Token pool PDA
    ///  11. `[writable]` Decompress destination
    ///  12. `[]` Token program
    ///  13. `[]` System program
    ///  14. `[writable]` State tree
    ///  15. `[writable]` Queue
    #[cfg(not(target_os = "solana"))]
    #[allow(clippy::too_many_arguments)]
    pub fn build_claim_and_decompress_instruction(
        accounts: &ClaimAccounts,
        proof: Option<CompressedProof>,
        root_index: u16,
        merkle_context: PackedMerkleContext,
        amount: u64,
        lamports: Option<u64>,
        mint: Pubkey,
        unlock_slot: u64,
        bump_seed: u8,
    ) -> Instruction {
        let accounts = vec![
            AccountMeta::new(accounts.claimant, true),
            AccountMeta::new(accounts.fee_payer, true),
            AccountMeta::new_readonly(accounts.associated_airdrop_pda, false),
            AccountMeta::new_readonly(accounts.ctoken_cpi_authority_pda, false),
            AccountMeta::new_readonly(accounts.light_system_program, false),
            AccountMeta::new_readonly(accounts.registered_program_pda, false),
            AccountMeta::new_readonly(accounts.noop_program, false),
            AccountMeta::new_readonly(accounts.account_compression_authority, false),
            AccountMeta::new_readonly(accounts.account_compression_program, false),
            AccountMeta::new_readonly(accounts.ctoken_program, false),
            AccountMeta::new(accounts.token_pool_pda, false),
            AccountMeta::new(accounts.decompress_destination, false),
            AccountMeta::new_readonly(accounts.token_program, false),
            AccountMeta::new_readonly(accounts.system_program, false),
            AccountMeta::new(accounts.state_tree, false),
            AccountMeta::new(accounts.queue, false),
        ];

        let instruction_data = ClaimProgramInstruction::ClaimAndDecompress {
            proof,
            root_index,
            merkle_context,
            amount,
            lamports,
            mint,
            unlock_slot,
            bump_seed,
        };

        Instruction {
            program_id: crate::id(),
            accounts,
            data: borsh::to_vec(&instruction_data).unwrap(),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_build_claim() {
            let accounts = ClaimAccounts {
                claimant: Pubkey::new_unique(),
                fee_payer: Pubkey::new_unique(),
                associated_airdrop_pda: Pubkey::new_unique(),
                ctoken_cpi_authority_pda: Pubkey::new_unique(),
                light_system_program: Pubkey::new_unique(),
                registered_program_pda: Pubkey::new_unique(),
                noop_program: Pubkey::new_unique(),
                account_compression_authority: Pubkey::new_unique(),
                account_compression_program: Pubkey::new_unique(),
                ctoken_program: Pubkey::new_unique(),
                token_pool_pda: Pubkey::new_unique(),
                decompress_destination: Pubkey::new_unique(),
                token_program: Pubkey::new_unique(),
                system_program: Pubkey::new_unique(),
                state_tree: Pubkey::new_unique(),
                queue: Pubkey::new_unique(),
            };

            let mint = Pubkey::new_unique();
            let root_index = 42;
            let merkle_context = PackedMerkleContext::default();
            let amount = 1000;
            let lamports = Some(1000);
            let unlock_slot = 12345;
            let bump_seed = 1;

            let instruction = build_claim_and_decompress_instruction(
                &accounts,
                None,
                root_index,
                merkle_context,
                amount,
                lamports,
                mint,
                unlock_slot,
                bump_seed,
            );

            assert_eq!(instruction.accounts.len(), 16);
            assert_eq!(instruction.accounts[0].pubkey, accounts.claimant);
            assert!(instruction.accounts[0].is_signer);
            assert_eq!(instruction.accounts[1].pubkey, accounts.fee_payer);
            assert!(instruction.accounts[1].is_signer);
            assert!(!instruction.accounts[2].is_signer);

            // Verify instruction can be deserialized
            let deserialized: ClaimProgramInstruction =
                ClaimProgramInstruction::try_from_slice(&instruction.data).unwrap();
            match deserialized {
                ClaimProgramInstruction::ClaimAndDecompress {
                    amount: _amount,
                    lamports: _lamports,
                    mint: _mint,
                    root_index: _root_index,
                    merkle_context: _merkle_context,
                    unlock_slot: _unlock_slot,
                    bump_seed: _bump_seed,
                    ..
                } => {
                    assert_eq!(amount, _amount);
                    assert_eq!(lamports, _lamports);
                    assert_eq!(mint, _mint);
                    assert_eq!(root_index, _root_index);
                    assert_eq!(merkle_context, _merkle_context);
                    assert_eq!(unlock_slot, _unlock_slot);
                    assert_eq!(bump_seed, _bump_seed);
                }
            }
        }
    }
}
