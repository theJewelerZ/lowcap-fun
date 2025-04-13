use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, MintTo, Transfer, CloseAccount};
use anchor_spl::associated_token::AssociatedToken;

declare_id!("DMzPtouoz4k44QN7qrdLxW5K2dHMAaozLxjt1XEuPcCg");

#[program]
pub mod lowcapfun {
    use super::*;

    pub fn launch_token(
        ctx: Context<LaunchToken>,
        _decimals: u8,
        supply: u64,
        curve_type: u8,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        config.token_mint = ctx.accounts.token_mint.key();
        config.curve_type = curve_type;
        config.total_supply = supply;
        config.tokens_sold = 0;
        config.launch_timestamp = Clock::get()?.unix_timestamp;
        config.bump = ctx.bumps.config;

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.token_mint.to_account_info(),
                to: ctx.accounts.creator_token_account.to_account_info(),
                authority: ctx.accounts.mint_authority.clone(),
            },
        );

        token::mint_to(cpi_ctx, supply)?;
        Ok(())
    }

    pub fn buy_tokens(ctx: Context<BuySell>, amount: u64) -> Result<()> {
        let config = &mut ctx.accounts.config;
        if config.curve_type == 3 && !is_token_alive(config)? {
            return Err(ErrorCode::TokenSelfDestructed.into());
        }

        let price_per_token = get_price(config.curve_type, config.tokens_sold)?;
        let total_price = price_per_token.checked_mul(amount).unwrap();

        require!(
            ctx.accounts.buyer.lamports() >= total_price,
            ErrorCode::InsufficientFunds
        );

        **ctx.accounts.buyer.try_borrow_mut_lamports()? -= total_price;
        **ctx.accounts.creator.try_borrow_mut_lamports()? += total_price;

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault_token_account.to_account_info(),
                to: ctx.accounts.buyer_token_account.to_account_info(),
                authority: ctx.accounts.creator.to_account_info(),
            },
        );

        token::transfer(cpi_ctx, amount)?;
        config.tokens_sold += amount;
        Ok(())
    }

    pub fn sell_tokens(ctx: Context<BuySell>, amount: u64) -> Result<()> {
        let config = &mut ctx.accounts.config;

        let price_per_token = get_price(config.curve_type, config.tokens_sold)?;
        let total_refund = price_per_token.checked_mul(amount).unwrap();

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.buyer_token_account.to_account_info(),
                to: ctx.accounts.vault_token_account.to_account_info(),
                authority: ctx.accounts.buyer.to_account_info(),
            },
        );

        token::transfer(cpi_ctx, amount)?;

        **ctx.accounts.creator.try_borrow_mut_lamports()? -= total_refund;
        **ctx.accounts.buyer.try_borrow_mut_lamports()? += total_refund;

        config.tokens_sold = config.tokens_sold.saturating_sub(amount);
        Ok(())
    }

    pub fn nuke_token(ctx: Context<NukeToken>) -> Result<()> {
        let config = &ctx.accounts.config;
        require!(config.curve_type == 3, ErrorCode::NotTimeBomb);
        require!(!is_token_alive(config)?, ErrorCode::TokenStillAlive);

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            CloseAccount {
                account: ctx.accounts.token_mint.to_account_info(),
                destination: ctx.accounts.creator.to_account_info(),
                authority: ctx.accounts.mint_authority.to_account_info(),
            },
        );

        token::close_account(cpi_ctx)?;
        Ok(())
    }
}

fn is_token_alive(config: &BondingConfig) -> Result<bool> {
    let current_time = Clock::get()?.unix_timestamp;
    let duration = current_time - config.launch_timestamp;
    Ok(duration < 86400 || config.tokens_sold * 100 / config.total_supply >= 70)
}

#[derive(Accounts)]
pub struct LaunchToken<'info> {
    #[account(init, payer = creator, mint::decimals = 9, mint::authority = mint_authority, mint::freeze_authority = mint_authority)]
    pub token_mint: Account<'info, Mint>,
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(init, payer = creator, associated_token::mint = token_mint, associated_token::authority = creator)]
    pub creator_token_account: Account<'info, TokenAccount>,
    /// CHECK:
    pub mint_authority: AccountInfo<'info>,
    #[account(init, seeds = [b"config", creator.key().as_ref(), token_mint.key().as_ref()], bump, payer = creator, space = 8 + 32 + 8 + 1 + 1 + 8 + 8)]
    pub config: Account<'info, BondingConfig>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct BuySell<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,
    /// CHECK: This is the creator’s wallet and is only used for lamport transfers, no data access or CPI invoked
    #[account(mut)]
    pub creator: AccountInfo<'info>,
    #[account(mut)]
    pub buyer_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,
    #[account(mut, seeds = [b"config", creator.key().as_ref(), config.token_mint.as_ref()], bump = config.bump)]
    pub config: Account<'info, BondingConfig>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct NukeToken<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,
    #[account(mut)]
    pub config: Account<'info, BondingConfig>,
    /// CHECK:
    pub mint_authority: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct BondingConfig {
    pub token_mint: Pubkey,
    pub total_supply: u64,
    pub curve_type: u8,
    pub bump: u8,
    pub tokens_sold: u64,
    pub launch_timestamp: i64,
}

pub fn get_price(curve_type: u8, tokens_sold: u64) -> Result<u64> {
    match curve_type {
        0 => Ok(1_000 + tokens_sold / 1_000),
        1 => Ok((1_000 * (105u64.pow((tokens_sold / 1_000) as u32))) / (100u64.pow((tokens_sold / 1_000) as u32))),
        2 => Ok(1_000u64.saturating_sub(tokens_sold / 10)),
        3 => Ok(1_000),
        _ => Err(ErrorCode::InvalidCurveType.into()),
    }
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid bonding curve type.")]
    InvalidCurveType,
    #[msg("Insufficient funds.")]
    InsufficientFunds,
    #[msg("This token has self-destructed.")]
    TokenSelfDestructed,
    #[msg("Token is still alive and can't be nuked.")]
    TokenStillAlive,
    #[msg("This token is not a Time Bomb.")]
    NotTimeBomb,
}

