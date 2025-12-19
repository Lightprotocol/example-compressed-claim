use borsh::{BorshDeserialize, BorshSerialize};
use light_compressed_account::compressed_account::PackedMerkleContext;
use light_ctoken_sdk::ValidityProof;
use solana_program::pubkey::Pubkey;

#[cfg(not(target_os = "solana"))]
use solana_program::instruction::{AccountMeta, Instruction};

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub enum ClaimProgramInstruction {
    Claim {
        validity_proof: ValidityProof,
        merkle_context: PackedMerkleContext,
        root_index: u16,
        amount: u64,
        mint: Pubkey,
        unlock_slot: u64,
        bump_seed: u8,
        // Indices into packed_accounts (accounts[10..])
        owner_index: u8,
        mint_index: u8,
        destination_index: u8,
        pool_account_index: u8,
        pool_index: u8,
        pool_bump: u8,
    },
}

/// Accounts for the claim instruction.
///
/// Account layout:
///   0. `[signer]` Claimant
///   1. `[signer]` Fee payer
///   2. `[]` Associated airdrop PDA (authority for compressed tokens)
///   3. `[]` CToken program
///   4. `[]` CToken CPI authority PDA
///   5. `[]` Light system program
///   6. `[]` Registered program PDA
///   7. `[]` Account compression authority
///   8. `[]` Account compression program
///   9. `[]` System program
///  10+: Packed accounts (merkle tree, queue, mint, owner, destination, pool, etc.)
#[cfg(not(target_os = "solana"))]
#[derive(Debug)]
pub struct ClaimAccounts {
    pub claimant: Pubkey,
    pub fee_payer: Pubkey,
    pub associated_airdrop_pda: Pubkey,
    pub ctoken_program: Pubkey,
    pub ctoken_cpi_authority_pda: Pubkey,
    pub light_system_program: Pubkey,
    pub registered_program_pda: Pubkey,
    pub account_compression_authority: Pubkey,
    pub account_compression_program: Pubkey,
    pub system_program: Pubkey,
    /// Packed accounts: merkle tree, queue, mint, owner, destination SPL account, token pool
    pub packed_accounts: Vec<(Pubkey, bool, bool)>, // (pubkey, is_signer, is_writable)
}

/// Build a claim instruction in the client.
#[cfg(not(target_os = "solana"))]
#[allow(clippy::too_many_arguments)]
pub fn build_claim_and_decompress_instruction(
    accounts: &ClaimAccounts,
    validity_proof: ValidityProof,
    merkle_context: PackedMerkleContext,
    root_index: u16,
    amount: u64,
    mint: Pubkey,
    unlock_slot: u64,
    bump_seed: u8,
    owner_index: u8,
    mint_index: u8,
    destination_index: u8,
    pool_account_index: u8,
    pool_index: u8,
    pool_bump: u8,
) -> Instruction {
    let mut account_metas = vec![
        AccountMeta::new(accounts.claimant, true),
        AccountMeta::new(accounts.fee_payer, true),
        AccountMeta::new_readonly(accounts.associated_airdrop_pda, false),
        AccountMeta::new_readonly(accounts.ctoken_program, false),
        AccountMeta::new_readonly(accounts.ctoken_cpi_authority_pda, false),
        AccountMeta::new_readonly(accounts.light_system_program, false),
        AccountMeta::new_readonly(accounts.registered_program_pda, false),
        AccountMeta::new_readonly(accounts.account_compression_authority, false),
        AccountMeta::new_readonly(accounts.account_compression_program, false),
        AccountMeta::new_readonly(accounts.system_program, false),
    ];

    // Add packed accounts
    for (pubkey, is_signer, is_writable) in &accounts.packed_accounts {
        if *is_writable {
            account_metas.push(AccountMeta::new(*pubkey, *is_signer));
        } else {
            account_metas.push(AccountMeta::new_readonly(*pubkey, *is_signer));
        }
    }

    let instruction_data = ClaimProgramInstruction::Claim {
        validity_proof,
        merkle_context,
        root_index,
        amount,
        mint,
        unlock_slot,
        bump_seed,
        owner_index,
        mint_index,
        destination_index,
        pool_account_index,
        pool_index,
        pool_bump,
    };

    Instruction {
        program_id: crate::id(),
        accounts: account_metas,
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
            ctoken_program: Pubkey::new_unique(),
            ctoken_cpi_authority_pda: Pubkey::new_unique(),
            light_system_program: Pubkey::new_unique(),
            registered_program_pda: Pubkey::new_unique(),
            account_compression_authority: Pubkey::new_unique(),
            account_compression_program: Pubkey::new_unique(),
            system_program: Pubkey::new_unique(),
            packed_accounts: vec![
                (Pubkey::new_unique(), false, true),  // merkle tree
                (Pubkey::new_unique(), false, true),  // queue
                (Pubkey::new_unique(), false, false), // mint
                (Pubkey::new_unique(), false, false), // owner (airdrop PDA)
                (Pubkey::new_unique(), false, true),  // destination SPL account
                (Pubkey::new_unique(), false, true),  // token pool
            ],
        };

        let mint = Pubkey::new_unique();
        let root_index = 42;
        let merkle_context = PackedMerkleContext::default();
        let amount = 1000;
        let unlock_slot = 12345;
        let bump_seed = 1;

        let instruction = build_claim_and_decompress_instruction(
            &accounts,
            ValidityProof::new(None),
            merkle_context,
            root_index,
            amount,
            mint,
            unlock_slot,
            bump_seed,
            3,  // owner_index
            2,  // mint_index
            4,  // destination_index
            5,  // pool_account_index
            0,  // pool_index
            255, // pool_bump
        );

        // 10 fixed accounts + 6 packed accounts = 16
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
            ClaimProgramInstruction::Claim {
                amount: _amount,
                mint: _mint,
                root_index: _root_index,
                merkle_context: _merkle_context,
                unlock_slot: _unlock_slot,
                bump_seed: _bump_seed,
                ..
            } => {
                assert_eq!(amount, _amount);
                assert_eq!(mint, _mint);
                assert_eq!(root_index, _root_index);
                assert_eq!(merkle_context, _merkle_context);
                assert_eq!(unlock_slot, _unlock_slot);
                assert_eq!(bump_seed, _bump_seed);
            }
        }
    }
}
