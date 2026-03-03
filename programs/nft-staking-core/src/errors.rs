use anchor_lang::error_code;

#[error_code]
pub enum StakingError {
    #[msg("NFT owner key mismatch")]
    InvalidOwner,
    #[msg("Invalid update authority")]
    InvalidAuthority,
    #[msg("NFT already staked")]
    AlreadyStaked,
    #[msg("NFT not staked")]
    NotStaked,
    #[msg("Invalid timestamp value")]
    InvalidTimestamp,
    #[msg("NFT freeze period not elapsed")]
    FreezePeriodNotElapsed,
    #[msg("Overflow")]
    Overflow,
    #[msg("No rewards available to claim")]
    NoRewardsToClaim,
    #[msg("Missing staking attribute")]
    MissingStakeAttribute,
    #[msg("Oracle transfer state is stale")]
    OracleStateStale,
    #[msg("Invalid oracle account")]
    InvalidOracleAccount,
    #[msg("Invalid new owner")]
    InvalidNewOwner,
    #[msg("Insufficient oracle vault balance")]
    InsufficientOracleVaultBalance,
    #[msg("Numerical underflow")]
    Underflow,
}
