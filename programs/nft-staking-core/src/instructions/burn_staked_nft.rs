use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{mint_to_checked, Mint, MintToChecked, TokenAccount, TokenInterface},
};
use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    fetch_plugin,
    instructions::BurnV1CpiBuilder,
    types::{Attributes, PluginType, UpdateAuthority},
    ID as MPL_CORE_ID,
};

use super::helpers::{
    calculate_days_elapsed, get_attribute, parse_i64_attribute, sync_collection_total_staked,
};
use crate::{errors::StakingError, state::Config};

const BURN_BONUS_MULTIPLIER: u64 = 10;

#[derive(Accounts)]
pub struct BurnStakedNft<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    /// CHECK: PDA update authority
    #[account(
        seeds = [b"update_authority", collection.key().as_ref()],
        bump
    )]
    pub update_authority: UncheckedAccount<'info>,
    #[account(
        seeds = [b"config", collection.key().as_ref()],
        bump = config.config_bump
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [b"rewards", config.key().as_ref()],
        bump = config.rewards_bump
    )]
    pub rewards_mint: InterfaceAccount<'info, Mint>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = rewards_mint,
        associated_token::authority = user,
    )]
    pub user_rewards_ata: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: checked by mpl core
    #[account(mut)]
    pub nft: UncheckedAccount<'info>,
    /// CHECK: checked by mpl core
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: mpl core program id
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> BurnStakedNft<'info> {
    pub fn burn_staked_nft(&mut self, bumps: &BurnStakedNftBumps) -> Result<()> {
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

        let collection_key = self.collection.key();
        let signer_seeds = &[
            b"update_authority",
            collection_key.as_ref(),
            &[bumps.update_authority],
        ];

        let current_timestamp = Clock::get()?.unix_timestamp;

        let fetched_attributes = match fetch_plugin::<BaseAssetV1, Attributes>(
            &self.nft.to_account_info(),
            PluginType::Attributes,
        ) {
            Ok((_, attributes, _)) => attributes,
            Err(_) => return Err(StakingError::NotStaked.into()),
        };

        require!(
            get_attribute(&fetched_attributes.attribute_list, "staked") == Some("true"),
            StakingError::NotStaked
        );

        let staked_at = parse_i64_attribute(&fetched_attributes.attribute_list, "staked_at")?;
        let last_claimed_at = get_attribute(&fetched_attributes.attribute_list, "last_claimed_at")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(staked_at);

        let claimable_days = calculate_days_elapsed(last_claimed_at, current_timestamp)?;
        let base_amount = (claimable_days.max(0) as u64)
            .checked_mul(self.config.points_per_stake as u64)
            .ok_or(StakingError::Overflow)?;
        let bonus = base_amount
            .checked_mul(BURN_BONUS_MULTIPLIER)
            .ok_or(StakingError::Overflow)?;
        let total_amount = base_amount
            .checked_add(bonus)
            .ok_or(StakingError::Overflow)?;

        if total_amount > 0 {
            let config_seeds = &[
                b"config",
                collection_key.as_ref(),
                &[self.config.config_bump],
            ];
            let config_signer_seeds = &[&config_seeds[..]];

            let cpi_accounts = MintToChecked {
                mint: self.rewards_mint.to_account_info(),
                to: self.user_rewards_ata.to_account_info(),
                authority: self.config.to_account_info(),
            };
            let cpi_ctx = CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                cpi_accounts,
                config_signer_seeds,
            );
            mint_to_checked(cpi_ctx, total_amount, self.rewards_mint.decimals)?;
        }

        BurnV1CpiBuilder::new(&self.mpl_core_program.to_account_info())
            .asset(&self.nft.to_account_info())
            .collection(Some(&self.collection.to_account_info()))
            .payer(&self.user.to_account_info())
            .authority(Some(&self.update_authority.to_account_info()))
            .system_program(Some(&self.system_program.to_account_info()))
            .invoke_signed(&[signer_seeds])?;

        sync_collection_total_staked(
            &self.mpl_core_program.to_account_info(),
            &self.collection.to_account_info(),
            &self.user.to_account_info(),
            &self.update_authority.to_account_info(),
            &self.system_program.to_account_info(),
            &[signer_seeds],
            -1,
        )?;

        Ok(())
    }
}
