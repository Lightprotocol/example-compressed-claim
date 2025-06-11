use crate::{
    instruction::ClaimProgramInstruction,
    instructions::claim_and_decompress::process_claim_and_decompress,
};
use borsh::BorshDeserialize;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = ClaimProgramInstruction::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    match instruction {
        ClaimProgramInstruction::ClaimAndDecompress {
            proof,
            root_index,
            merkle_context,
            amount,
            lamports,
            mint,
            unlock_slot,
            bump_seed,
        } => process_claim_and_decompress(
            program_id,
            accounts,
            proof,
            root_index,
            merkle_context,
            amount,
            lamports,
            mint,
            unlock_slot,
            bump_seed,
        ),
    }
}
