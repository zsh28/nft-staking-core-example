use anchor_lang::prelude::*;
use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    instructions::TransferV1CpiBuilder,
    types::UpdateAuthority,
    ID as MPL_CORE_ID,
};

use crate::{
    errors::StakingError,
    state::{OracleState, ORACLE_STATE_SEED},
};

#[derive(Accounts)]
pub struct TransferNft<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    /// CHECK: destination owner
    pub new_owner: UncheckedAccount<'info>,
    /// CHECK: checked by mpl core
    #[account(mut)]
    pub nft: UncheckedAccount<'info>,
    /// CHECK: checked by mpl core
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: PDA update authority
    #[account(
        seeds = [b"update_authority", collection.key().as_ref()],
        bump
    )]
    pub update_authority: UncheckedAccount<'info>,
    #[account(
        seeds = [ORACLE_STATE_SEED, collection.key().as_ref()],
        bump = oracle_state.bump
    )]
    pub oracle_state: Account<'info, OracleState>,
    /// CHECK: mpl core id
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> TransferNft<'info> {
    pub fn transfer_nft(&mut self) -> Result<()> {
        require!(
            self.new_owner.key() != Pubkey::default(),
            StakingError::InvalidNewOwner
        );

        let base_asset = BaseAssetV1::try_from(&self.nft.to_account_info())?;
        require!(
            base_asset.owner == self.user.key(),
            StakingError::InvalidOwner
        );
        require!(
            base_asset.update_authority == UpdateAuthority::Collection(self.collection.key()),
            StakingError::InvalidAuthority
        );

        let base_collection = BaseCollectionV1::try_from(&self.collection.to_account_info())?;
        require!(
            base_collection.update_authority == self.update_authority.key(),
            StakingError::InvalidAuthority
        );

        TransferV1CpiBuilder::new(&self.mpl_core_program.to_account_info())
            .asset(&self.nft.to_account_info())
            .collection(Some(&self.collection.to_account_info()))
            .payer(&self.user.to_account_info())
            .authority(Some(&self.user.to_account_info()))
            .new_owner(&self.new_owner.to_account_info())
            .system_program(Some(&self.system_program.to_account_info()))
            .add_remaining_account(&self.oracle_state.to_account_info(), false, false)
            .invoke()?;

        Ok(())
    }
}
