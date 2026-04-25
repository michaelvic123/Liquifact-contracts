use super::{
    external_calls, LiquifactEscrow, LiquifactEscrowClient, YieldTier, MAX_DUST_SWEEP_AMOUNT,
    SCHEMA_VERSION,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, Env, String, Vec as SorobanVec,
};

// Focused test tree for escrow behavior. Shared helpers live here so feature
// modules stay assertion-focused and each test still owns a fresh Env.
mod admin;
mod funding;
mod init;
mod integration;
mod properties;
mod settlement;

pub(super) fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

pub(super) fn setup(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    (client, admin, sme)
}

pub(super) fn free_addresses(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

pub(super) fn default_init(
    client: &LiquifactEscrowClient<'_>,
    env: &Env,
    admin: &Address,
    sme: &Address,
) {
    let (token, treasury) = free_addresses(env);
    client.init(
        admin,
        &String::from_str(env, "INV001"),
        sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
}

/// Helper to create a realistic USDC-style escrow initialization for integration tests.
/// Uses 7 decimal places (10,000,000 base units = 1 USDC) and reasonable parameters.
pub(super) fn setup_realistic_usdc_escrow(
    client: &LiquifactEscrowClient<'_>,
    env: &Env,
    admin: &Address,
    sme: &Address,
    target_usdc: i128,
    yield_bps: i64,
    maturity_secs: u64,
) -> (Address, Address) {
    let (funding_token, treasury) = free_addresses(env);
    let usdc_decimals = 10_000_000i128; // 7 decimals
    let target_base_units = target_usdc * usdc_decimals;
    
    client.init(
        admin,
        &String::from_str(env, "USDC001"),
        sme,
        &target_base_units,
        &yield_bps,
        &maturity_secs,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    
    (funding_token, treasury)
}

/// Helper to create multiple test investors with generated addresses.
pub(super) fn create_test_investors(env: &Env, count: usize) -> Vec<Address> {
    (0..count).map(|_| Address::generate(env)).collect()
}

/// Helper to advance ledger time for maturity testing.
pub(super) fn advance_time_to_maturity(env: &Env, maturity_secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = maturity_secs + 1;
    });
}

pub(super) const TARGET: i128 = 10_000_0000000i128;
