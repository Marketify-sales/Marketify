use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer, CloseAccount};
use anchor_spl::{associated_token, associated_token::AssociatedToken};

declare_id!("FSoNXDpgsYZkp3VtPjPWPR2cQ5PMPt16SmLFm75A7FYh");

#[program]
pub mod marketify {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, fee_percentage: u8) -> Result<()> {
        require!(
            fee_percentage <= 10,
            MarketplaceError::FeeTooHigh
        );

        let marketplace = &mut ctx.accounts.marketplace;
        marketplace.authority = ctx.accounts.authority.key();
        marketplace.fee_percentage = fee_percentage;
        marketplace.treasury = ctx.accounts.treasury_account.key();

        msg!("Marketplace initialized with fee: {}%", fee_percentage);
        Ok(())
    }

    pub fn list_product(ctx: Context<ListProduct>, price: u64) -> Result<()> {
        require!(price > 0, MarketplaceError::InvalidPrice);

        let listing = &mut ctx.accounts.listing;
        listing.seller = ctx.accounts.seller.key();
        listing.nft_mint = ctx.accounts.nft_mint.key();
        listing.price = price;
        // Create associated token account for the listing PDA (escrow)
        let cpi_accounts = associated_token::Create {
            payer: ctx.accounts.seller.to_account_info(),
            associated_token: ctx.accounts.escrow_token_account.to_account_info(),
            authority: ctx.accounts.listing.to_account_info(),
            mint: ctx.accounts.nft_mint.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            token_program: ctx.accounts.token_program.to_account_info(),
            rent: ctx.accounts.rent.to_account_info(),
        };
        associated_token::create(CpiContext::new(
            ctx.accounts.associated_token_program.to_account_info(),
            cpi_accounts,
        ))?;

        // Transfer 1 NFT from seller -> escrow (associated token account owned by listing PDA)
        let cpi_accounts_transfer = token::Transfer {
            from: ctx.accounts.seller_token_account.to_account_info(),
            to: ctx.accounts.escrow_token_account.to_account_info(),
            authority: ctx.accounts.seller.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts_transfer);
        token::transfer(cpi_ctx, 1)?;

        listing.seller_token_account = ctx.accounts.escrow_token_account.key();
        listing.is_sold = false;

        msg!("Product listed: NFT {} at price {} lamports (escrow: {})", listing.nft_mint, price, listing.seller_token_account);
        Ok(())
    }

    pub fn buy_product(ctx: Context<BuyProduct>) -> Result<()> {
        let listing = &ctx.accounts.listing;
        let marketplace = &ctx.accounts.marketplace;

        require!(!listing.is_sold, MarketplaceError::AlreadySold);
        require!(listing.price > 0, MarketplaceError::InvalidPrice);

        let buyer = &ctx.accounts.buyer;
        let seller = &ctx.accounts.seller;
        let treasury = &ctx.accounts.treasury;

        // Calculate fee
        let fee_amount = (listing.price as u128)
            .checked_mul(marketplace.fee_percentage as u128)
            .and_then(|x| x.checked_div(100))
            .ok_or(MarketplaceError::MathOverflow)? as u64;

        let seller_amount = listing
            .price
            .checked_sub(fee_amount)
            .ok_or(MarketplaceError::MathOverflow)?;

        // Transfer SOL to seller (after fee deduction)
        anchor_lang::solana_program::program::invoke(
            &anchor_lang::solana_program::system_instruction::transfer(
                buyer.key,
                seller.key,
                seller_amount,
            ),
            &[
                buyer.to_account_info(),
                seller.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        // Transfer fee to treasury
        anchor_lang::solana_program::program::invoke(
            &anchor_lang::solana_program::system_instruction::transfer(
                buyer.key,
                treasury.key,
                fee_amount,
            ),
            &[
                buyer.to_account_info(),
                treasury.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        // Transfer NFT from escrow (listing PDA) to buyer
        let listing_bump = *ctx.bumps.get("listing").ok_or(MarketplaceError::MathOverflow)?;
        let seeds = &[b"listing", listing.nft_mint.as_ref(), &[listing_bump]];
        let signer_seeds = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.escrow_token_account.to_account_info(),
            to: ctx.accounts.buyer_token_account.to_account_info(),
            authority: ctx.accounts.listing.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, 1)?;

        // Close escrow token account and send lamports to seller
        let cpi_accounts_close = CloseAccount {
            account: ctx.accounts.escrow_token_account.to_account_info(),
            destination: ctx.accounts.seller.to_account_info(),
            authority: ctx.accounts.listing.to_account_info(),
        };
        let cpi_ctx_close = CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), cpi_accounts_close, signer_seeds);
        token::close_account(cpi_ctx_close)?;

        // Mark listing as sold
        let listing_account = &mut ctx.accounts.listing;
        listing_account.is_sold = true;

        msg!(
            "Product sold: NFT {} for {} lamports (fee: {} lamports)",
            listing.nft_mint,
            listing.price,
            fee_amount
        );

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(fee_percentage: u8)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + Marketplace::INIT_SPACE,
        seeds = [b"marketplace"],
        bump
    )]
    pub marketplace: Account<'info, Marketplace>,

    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: Treasury account that will receive fees
    #[account(mut)]
    pub treasury_account: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(price: u64)]
pub struct ListProduct<'info> {
    #[account(
        init,
        payer = seller,
        space = 8 + Listing::INIT_SPACE,
        seeds = [b"listing", nft_mint.key().as_ref()],
        bump
    )]
    pub listing: Account<'info, Listing>,

    #[account(mut)]
    pub seller: Signer<'info>,

    /// CHECK: NFT mint account
    pub nft_mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = seller_token_account.owner == seller.key(),
        constraint = seller_token_account.mint == nft_mint.key(),
        constraint = seller_token_account.amount >= 1
    )]
    pub seller_token_account: Account<'info, TokenAccount>,

    /// CHECK: Escrow token account (ATA for listing PDA)
    #[account(mut)]
    pub escrow_token_account: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct BuyProduct<'info> {
    #[account(
        mut,
        seeds = [b"listing", listing.nft_mint.as_ref()],
        bump,
        constraint = !listing.is_sold @ MarketplaceError::AlreadySold,
        close = seller
    )]
    pub listing: Account<'info, Listing>,

    #[account(
        seeds = [b"marketplace"],
        bump
    )]
    pub marketplace: Account<'info, Marketplace>,

    #[account(mut)]
    pub buyer: Signer<'info>,

    /// CHECK: Seller account
    #[account(
        mut,
        constraint = seller.key() == listing.seller @ MarketplaceError::InvalidSeller
    )]
    pub seller: UncheckedAccount<'info>,

    /// CHECK: Treasury account
    #[account(
        mut,
        constraint = treasury.key() == marketplace.treasury @ MarketplaceError::InvalidTreasury
    )]
    pub treasury: UncheckedAccount<'info>,

    #[account(
        mut,
        constraint = escrow_token_account.owner == listing.key() @ MarketplaceError::InvalidSeller,
        constraint = escrow_token_account.mint == listing.nft_mint,
        constraint = escrow_token_account.amount >= 1
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = buyer_token_account.owner == buyer.key(),
        constraint = buyer_token_account.mint == listing.nft_mint
    )]
    pub buyer_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,

    /// CHECK: System program for SOL transfers
    pub system_program: Program<'info, System>,
}

#[account]
#[derive(InitSpace)]
pub struct Marketplace {
    pub authority: Pubkey,
    pub fee_percentage: u8,
    pub treasury: Pubkey,
}

#[account]
#[derive(InitSpace)]
pub struct Listing {
    pub seller: Pubkey,
    pub nft_mint: Pubkey,
    pub price: u64,
    pub seller_token_account: Pubkey,
    pub escrow_token_account: Pubkey,
    pub is_sold: bool,
}

#[error_code]
pub enum MarketplaceError {
    #[msg("Fee percentage cannot exceed 10%")]
    FeeTooHigh,
    #[msg("Product has already been sold")]
    AlreadySold,
    #[msg("Invalid price")]
    InvalidPrice,
    #[msg("Math overflow occurred")]
    MathOverflow,
    #[msg("Invalid seller account")]
    InvalidSeller,
    #[msg("Invalid treasury account")]
    InvalidTreasury,
}

