use solana_program::program_error::ProgramError;
use thiserror::Error;

#[derive(Error, Debug, Copy, Clone)]
pub enum CustomError {
    #[error("Missing required signature.")]
    MissingRequiredSignature,
    #[error("Invalid instruction data length; expected 8 bytes.")]
    InvalidInstructionDataLength,
    #[error("Tokens are still locked.")]
    TokensLocked,
    #[error("Invalid airdrop PDA provided.")]
    InvalidPDA,
}

impl From<CustomError> for ProgramError {
    fn from(e: CustomError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
