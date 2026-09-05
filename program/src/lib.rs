#![no_std]
#![allow(unexpected_cfgs)]

use pinocchio::{
    cpi::{Seed, Signer},
    default_allocator, nostd_panic_handler, program_entrypoint,
    error::ProgramError,
    sysvars::{clock::Clock, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio_system::instructions::{CreateAccount, Transfer};

program_entrypoint!(process_instruction);
default_allocator!();
nostd_panic_handler!();

const MARKET_LEN: usize = 160;
const POSITION_LEN: usize = 80;
const MARKET_MAGIC: &[u8; 8] = b"WEATHER1";
const POSITION_MAGIC: &[u8; 8] = b"POSITION";
const MAX_OBSERVATION_DRIFT: i64 = 86_400;

#[repr(u32)]
enum WeatherError {
    InvalidInstruction = 1,
    MissingAccount,
    InvalidSigner,
    InvalidWritable,
    InvalidOwner,
    InvalidPda,
    AlreadyInitialized,
    InvalidTime,
    MarketClosed,
    NotClosed,
    AlreadySettled,
    NotSettled,
    Unauthorized,
    InvalidAmount,
    MathOverflow,
    AlreadyClaimed,
    LosingPosition,
    RefundUnavailable,
    InvalidState,
}

fn err(e: WeatherError) -> ProgramError {
    ProgramError::Custom(e as u32)
}
fn read_u64(d: &[u8], o: usize) -> Result<u64, ProgramError> {
    d.get(o..o + 8)
        .and_then(|x| x.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(err(WeatherError::InvalidInstruction))
}
fn read_i64(d: &[u8], o: usize) -> Result<i64, ProgramError> {
    d.get(o..o + 8)
        .and_then(|x| x.try_into().ok())
        .map(i64::from_le_bytes)
        .ok_or(err(WeatherError::InvalidInstruction))
}
fn put_u64(d: &mut [u8], o: usize, v: u64) {
    d[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
fn put_i64(d: &mut [u8], o: usize, v: i64) {
    d[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
fn require(a: &AccountView, signer: bool, writable: bool) -> ProgramResult {
    if signer && !a.is_signer() {
        return Err(err(WeatherError::InvalidSigner));
    }
    if writable && !a.is_writable() {
        return Err(err(WeatherError::InvalidWritable));
    }
    Ok(())
}
fn assert_pda(
    address: &Address,
    seeds: &[&[u8]],
    program_id: &Address,
) -> Result<u8, ProgramError> {
    let (expected, bump) = Address::find_program_address(seeds, program_id);
    if &expected != address {
        return Err(err(WeatherError::InvalidPda));
    }
    Ok(bump)
}
fn market_ok(a: &AccountView, pid: &Address) -> ProgramResult {
    if a.owner() != pid || a.data_len() != MARKET_LEN {
        return Err(err(WeatherError::InvalidOwner));
    }
    let d = a.try_borrow()?;
    if d.get(..8) != Some(MARKET_MAGIC) {
        return Err(err(WeatherError::InvalidState));
    }
    Ok(())
}

pub fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    ix: &[u8],
) -> ProgramResult {
    match ix.first().copied() {
        Some(0) => create_market(program_id, accounts, ix),
        Some(1) => place_bet(program_id, accounts, ix),
        Some(2) => close_market(program_id, accounts),
        Some(3) => settle_market(program_id, accounts, ix),
        Some(4) => claim(program_id, accounts),
        Some(5) => refund(program_id, accounts),
        _ => Err(err(WeatherError::InvalidInstruction)),
    }
}

// Market: magic 0..8, authority 8..40, oracle 40..72, target_day 72,
// close_ts 80, observation_ts 88, threshold_um 96, yes 104, no 112,
// observed_um 120, settled_at 128, status 136, outcome 137, bump 138, vault_bump 139.
fn create_market(pid: &Address, a: &mut [AccountView], ix: &[u8]) -> ProgramResult {
    if a.len() != 4 || ix.len() != 66 {
        return Err(err(WeatherError::InvalidInstruction));
    }
    let (payer_s, rest) = a.split_at_mut(1);
    let payer = &payer_s[0];
    let (market_s, rest) = rest.split_at_mut(1);
    let market = &mut market_s[0];
    let (vault_s, _) = rest.split_at_mut(1);
    let vault = &vault_s[0];
    require(payer, true, true)?;
    require(market, false, true)?;
    require(vault, false, true)?;
    if market.data_len() != 0 || vault.data_len() != 0 {
        return Err(err(WeatherError::AlreadyInitialized));
    }
    let target = read_i64(ix, 1)?;
    let close = read_i64(ix, 9)?;
    let obs = read_i64(ix, 17)?;
    let threshold = read_u64(ix, 25)?;
    let now = Clock::get()?.unix_timestamp;
    if close <= now || target <= close || obs < target || threshold == 0 {
        return Err(err(WeatherError::InvalidTime));
    }
    let bump = assert_pda(
        market.address(),
        &[b"market", payer.address().as_ref(), &target.to_le_bytes()],
        pid,
    )?;
    let vbump = assert_pda(vault.address(), &[b"vault", market.address().as_ref()], pid)?;
    let bump_ref = [bump];
    let target_bytes = target.to_le_bytes();
    let seeds = [
        Seed::from(b"market"),
        Seed::from(payer.address().as_ref()),
        Seed::from(&target_bytes),
        Seed::from(&bump_ref),
    ];
    CreateAccount::with_minimum_balance(payer, market, MARKET_LEN as u64, pid, None)?
        .invoke_signed(&[Signer::from(&seeds)])?;
    let vbump_ref = [vbump];
    let vseeds = [
        Seed::from(b"vault"),
        Seed::from(market.address().as_ref()),
        Seed::from(&vbump_ref),
    ];
    CreateAccount::with_minimum_balance(payer, vault, 0, pid, None)?
        .invoke_signed(&[Signer::from(&vseeds)])?;
    let mut d = market.try_borrow_mut()?;
    d[..8].copy_from_slice(MARKET_MAGIC);
    d[8..40].copy_from_slice(payer.address().as_ref());
    d[40..72].copy_from_slice(&ix[33..65]);
    put_i64(&mut d, 72, target);
    put_i64(&mut d, 80, close);
    put_i64(&mut d, 88, obs);
    put_u64(&mut d, 96, threshold);
    d[136] = 0;
    d[138] = bump;
    d[139] = vbump;
    Ok(())
}

// Position: magic, market 8..40, user 40..72, amount 72..80, side encoded in high bit; claimed uses second-high bit.
fn place_bet(pid: &Address, a: &mut [AccountView], ix: &[u8]) -> ProgramResult {
    if a.len() != 5 || ix.len() != 10 {
        return Err(err(WeatherError::InvalidInstruction));
    }
    let side = ix[1];
    let amount = read_u64(ix, 2)?;
    if side > 1 || amount == 0 || amount >= (1u64 << 62) {
        return Err(err(WeatherError::InvalidAmount));
    }
    let (user_s, r) = a.split_at_mut(1);
    let user = &user_s[0];
    let (market_s, r) = r.split_at_mut(1);
    let market = &mut market_s[0];
    let (pos_s, r) = r.split_at_mut(1);
    let pos = &mut pos_s[0];
    let (vault_s, _) = r.split_at_mut(1);
    let vault = &vault_s[0];
    require(user, true, true)?;
    require(market, false, true)?;
    require(pos, false, true)?;
    require(vault, false, true)?;
    market_ok(market, pid)?;
    let now = Clock::get()?.unix_timestamp;
    {
        let d = market.try_borrow()?;
        if d[136] != 0 || now >= read_i64(&d, 80)? {
            return Err(err(WeatherError::MarketClosed));
        }
    }
    let pbump = assert_pda(
        pos.address(),
        &[
            b"position",
            market.address().as_ref(),
            user.address().as_ref(),
        ],
        pid,
    )?;
    assert_pda(vault.address(), &[b"vault", market.address().as_ref()], pid)?;
    if pos.data_len() == 0 {
        let br = [pbump];
        let seeds = [
            Seed::from(b"position"),
            Seed::from(market.address().as_ref()),
            Seed::from(user.address().as_ref()),
            Seed::from(&br),
        ];
        CreateAccount::with_minimum_balance(user, pos, POSITION_LEN as u64, pid, None)?
            .invoke_signed(&[Signer::from(&seeds)])?;
        let mut p = pos.try_borrow_mut()?;
        p[..8].copy_from_slice(POSITION_MAGIC);
        p[8..40].copy_from_slice(market.address().as_ref());
        p[40..72].copy_from_slice(user.address().as_ref());
        put_u64(&mut p, 72, (side as u64) << 63);
    }
    if pos.owner() != pid || pos.data_len() != POSITION_LEN {
        return Err(err(WeatherError::InvalidOwner));
    }
    {
        let mut p = pos.try_borrow_mut()?;
        if p[..8] != POSITION_MAGIC[..]
            || &p[8..40] != market.address().as_ref()
            || &p[40..72] != user.address().as_ref()
        {
            return Err(err(WeatherError::InvalidState));
        }
        let raw = read_u64(&p, 72)?;
        if (raw >> 63) as u8 != side || raw & (1u64 << 62) != 0 {
            return Err(err(WeatherError::InvalidState));
        }
        let total = (raw & ((1u64 << 62) - 1))
            .checked_add(amount)
            .ok_or(err(WeatherError::MathOverflow))?;
        put_u64(&mut p, 72, total | ((side as u64) << 63));
    }
    Transfer {
        from: user,
        to: vault,
        lamports: amount,
    }
    .invoke()?;
    let mut d = market.try_borrow_mut()?;
    let o = if side == 1 { 104 } else { 112 };
    let total = read_u64(&d, o)?
        .checked_add(amount)
        .ok_or(err(WeatherError::MathOverflow))?;
    put_u64(&mut d, o, total);
    Ok(())
}
fn close_market(pid: &Address, a: &mut [AccountView]) -> ProgramResult {
    if a.len() != 2 {
        return Err(err(WeatherError::MissingAccount));
    }
    let (authority_s, market_s) = a.split_at_mut(1);
    let authority = &authority_s[0];
    let market = &mut market_s[0];
    require(authority, true, false)?;
    require(market, false, true)?;
    market_ok(market, pid)?;
    let mut d = market.try_borrow_mut()?;
    if &d[8..40] != authority.address().as_ref() {
        return Err(err(WeatherError::Unauthorized));
    }
    if d[136] != 0 {
        return Err(err(WeatherError::InvalidState));
    }
    d[136] = 1;
    Ok(())
}
fn settle_market(pid: &Address, a: &mut [AccountView], ix: &[u8]) -> ProgramResult {
    if a.len() != 2 || ix.len() != 17 {
        return Err(err(WeatherError::InvalidInstruction));
    }
    let (oracle_s, market_s) = a.split_at_mut(1);
    let oracle = &oracle_s[0];
    let market = &mut market_s[0];
    require(oracle, true, false)?;
    require(market, false, true)?;
    market_ok(market, pid)?;
    let observed_ts = read_i64(ix, 1)?;
    let precip = read_u64(ix, 9)?;
    let now = Clock::get()?.unix_timestamp;
    let mut d = market.try_borrow_mut()?;
    if d[136] == 2 {
        return Err(err(WeatherError::AlreadySettled));
    }
    if now < read_i64(&d, 80)? && d[136] == 0 {
        return Err(err(WeatherError::NotClosed));
    }
    if &d[40..72] != oracle.address().as_ref() {
        return Err(err(WeatherError::Unauthorized));
    }
    let expected = read_i64(&d, 88)?;
    if observed_ts < expected - MAX_OBSERVATION_DRIFT
        || observed_ts > expected + MAX_OBSERVATION_DRIFT
        || observed_ts > now + 300
        || now - observed_ts > 172800
    {
        return Err(err(WeatherError::InvalidTime));
    }
    let threshold = read_u64(&d, 96)?;
    put_u64(&mut d, 120, precip);
    put_i64(&mut d, 128, now);
    d[136] = 2;
    d[137] = if precip >= threshold { 1 } else { 0 };
    Ok(())
}
fn payout(pid: &Address, a: &mut [AccountView], is_refund: bool) -> ProgramResult {
    if a.len() != 4 {
        return Err(err(WeatherError::MissingAccount));
    }
    let (user_s, r) = a.split_at_mut(1);
    let user = &user_s[0];
    let (market_s, r) = r.split_at_mut(1);
    let market = &market_s[0];
    let (pos_s, r) = r.split_at_mut(1);
    let pos = &mut pos_s[0];
    let vault = &r[0];
    require(user, true, true)?;
    require(market, false, false)?;
    require(pos, false, true)?;
    require(vault, false, true)?;
    market_ok(market, pid)?;
    assert_pda(
        pos.address(),
        &[
            b"position",
            market.address().as_ref(),
            user.address().as_ref(),
        ],
        pid,
    )?;
    let vb = assert_pda(vault.address(), &[b"vault", market.address().as_ref()], pid)?;
    if pos.owner() != pid {
        return Err(err(WeatherError::InvalidOwner));
    }
    let d = market.try_borrow()?;
    let mut p = pos.try_borrow_mut()?;
    let raw = read_u64(&p, 72)?;
    if raw & (1u64 << 62) != 0 {
        return Err(err(WeatherError::AlreadyClaimed));
    }
    let amount = raw & ((1u64 << 62) - 1);
    let side = (raw >> 63) as u8;
    let pay = if is_refund {
        let now = Clock::get()?.unix_timestamp;
        if d[136] == 2 || now < read_i64(&d, 88)?.saturating_add(172800) {
            return Err(err(WeatherError::RefundUnavailable));
        }
        amount
    } else {
        if d[136] != 2 {
            return Err(err(WeatherError::NotSettled));
        }
        if side != d[137] {
            return Err(err(WeatherError::LosingPosition));
        }
        let win = if side == 1 {
            read_u64(&d, 104)?
        } else {
            read_u64(&d, 112)?
        };
        let pool = read_u64(&d, 104)?
            .checked_add(read_u64(&d, 112)?)
            .ok_or(err(WeatherError::MathOverflow))?;
        (amount as u128)
            .checked_mul(pool as u128)
            .and_then(|x| x.checked_div(win as u128))
            .and_then(|x| u64::try_from(x).ok())
            .ok_or(err(WeatherError::MathOverflow))?
    };
    put_u64(&mut p, 72, raw | (1u64 << 62));
    drop(p);
    drop(d);
    let br = [vb];
    let seeds = [
        Seed::from(b"vault"),
        Seed::from(market.address().as_ref()),
        Seed::from(&br),
    ];
    Transfer {
        from: vault,
        to: user,
        lamports: pay,
    }
    .invoke_signed(&[Signer::from(&seeds)])
}
fn claim(pid: &Address, a: &mut [AccountView]) -> ProgramResult {
    payout(pid, a, false)
}
fn refund(pid: &Address, a: &mut [AccountView]) -> ProgramResult {
    payout(pid, a, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layouts_are_stable() {
        assert_eq!(MARKET_LEN, 160);
        assert_eq!(POSITION_LEN, 80);
    }
    #[test]
    fn decoding_rejects_short() {
        assert!(read_u64(&[0; 7], 0).is_err());
    }
}
