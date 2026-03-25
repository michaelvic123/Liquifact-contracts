//! # LiquiFact Escrow Contract - Tests
//!
//! This module contains comprehensive tests for the escrow contract,
//! including unit tests for all public functions and emergency refund functionality.

use super::{LiquifactEscrow, LiquifactEscrowClient, SCHEMA_VERSION, DataKey, InvoiceEscrow};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Map};

// ──────────────────────────────────────────────────────────────────────────────
// Test Setup Helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Deploy a new escrow contract and return the client
fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let contract_id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &contract_id)
}

/// Create a default escrow with admin, SME, and standard parameters
fn default_init(client: &LiquifactEscrowClient, admin: &Address, sme: &Address) {
    client.init(
        admin,
        &symbol_short!("INV001"),
        sme,
        &10_000_0000000i128, // amount
        &800u64,             // yield_bps
        &1000u64,            // maturity
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Initialization Tests
// ──────────────────────────────────────────────────────────────────────────────

/// After `init` the escrow must be open (status 0) with zero funded_amount,
/// emergency_mode false, and `get_escrow` must return an identical snapshot.
#[test]
fn test_init_creates_open_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let escrow = client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );

    // Verify initial state
    assert_eq!(escrow.status, 0);
    assert_eq!(escrow.funded_amount, 0);
    assert_eq!(escrow.emergency_mode, false);
    assert_eq!(escrow.admin, admin);
    assert_eq!(escrow.sme_address, sme);
    assert_eq!(escrow.amount, 10_000_0000000i128);
    assert_eq!(escrow.version, SCHEMA_VERSION);
}

/// Version should be set correctly during initialization
#[test]
fn test_init_sets_version() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );

    assert_eq!(client.get_version(), SCHEMA_VERSION);
}

/// Cannot re-initialize an already initialized escrow
#[test]
#[should_panic(expected = "Escrow already initialized")]
fn test_reinit_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
}

/// Zero amount should be rejected during initialization
#[test]
#[should_panic(expected = "Escrow amount must be positive")]
fn test_init_with_zero_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &0i128,
        &800u64,
        &1000u64,
    );
}

/// `get_escrow` must match what `init` returned
#[test]
fn test_get_escrow_matches_init() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let escrow = client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );

    let retrieved = client.get_escrow();
    assert_eq!(retrieved.invoice_id, escrow.invoice_id);
    assert_eq!(retrieved.admin, escrow.admin);
    assert_eq!(retrieved.sme_address, escrow.sme_address);
    assert_eq!(retrieved.amount, escrow.amount);
    assert_eq!(retrieved.funded_amount, escrow.funded_amount);
    assert_eq!(retrieved.status, escrow.status);
    assert_eq!(retrieved.emergency_mode, escrow.emergency_mode);
}

/// `get_escrow` panics before initialization
#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_get_escrow_uninitialized_panics() {
    let env = Env::default();
    let client = deploy(&env);
    client.get_escrow();
}

// ──────────────────────────────────────────────────────────────────────────────
// Funding Tests
// ──────────────────────────────────────────────────────────────────────────────

/// Successful funding updates funded_amount and investor balance
#[test]
fn test_fund_updates_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );

    let result = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(result.funded_amount, 5_000_0000000i128);
    assert_eq!(result.status, 0); // Still open, not fully funded
    
    // Check investor balance tracking
    let balance = client.get_investor_balance(&investor);
    assert_eq!(balance, 5_000_0000000i128);
}

/// Funding reaches target and transitions to funded status
#[test]
fn test_fund_reaches_target_transitions_to_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );

    let result = client.fund(&investor, &10_000_0000000i128);
    assert_eq!(result.funded_amount, 10_000_0000000i128);
    assert_eq!(result.status, 1); // Funded status
}

/// Cannot fund with zero amount
#[test]
#[should_panic(expected = "Funding amount must be positive")]
fn test_fund_with_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &0i128);
}

/// Cannot fund after status is funded (1)
#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128); // Now status = 1
    client.fund(&investor, &1i128); // Should fail
}

/// Cannot fund during emergency mode
#[test]
#[should_panic(expected = "Cannot fund while emergency mode is active")]
fn test_fund_during_emergency_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.activate_emergency();
    client.fund(&investor, &1i128);
}

/// Partial funding maintains open status
#[test]
fn test_partial_fund_stays_open() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );

    let partial = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(partial.status, 0);
    assert_eq!(partial.funded_amount, 5_000_0000000i128);

    let full = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(full.status, 1);
    assert_eq!(full.funded_amount, 10_000_0000000i128);
}

// ──────────────────────────────────────────────────────────────────────────────
// Settlement Tests
// ──────────────────────────────────────────────────────────────────────────────

/// Settlement transitions escrow to settled status
#[test]
fn test_settle_transitions_to_settled() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    
    let result = client.settle();
    assert_eq!(result.status, 2); // Settled status
}

/// Cannot settle before funded
#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_before_funded_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.settle();
}

/// Cannot settle during emergency mode
#[test]
#[should_panic(expected = "Cannot settle during emergency mode")]
fn test_settle_during_emergency_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    client.activate_emergency();
    client.settle();
}

// ──────────────────────────────────────────────────────────────────────────────
// Update Maturity Tests
// ──────────────────────────────────────────────────────────────────────────────

/// Admin can update maturity in open state
#[test]
fn test_update_maturity_by_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );

    let result = client.update_maturity(&2000u64);
    assert_eq!(result.maturity, 2000u64);
}

/// Cannot update maturity after funding
#[test]
#[should_panic(expected = "Maturity can only be updated in Open state")]
fn test_update_maturity_after_funding_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    client.update_maturity(&2000u64);
}

/// Cannot update maturity during emergency mode
#[test]
#[should_panic(expected = "Cannot update maturity during emergency mode")]
fn test_update_maturity_during_emergency_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.activate_emergency();
    client.update_maturity(&2000u64);
}

// ──────────────────────────────────────────────────────────────────────────────
// Emergency Mode Tests
// ──────────────────────────────────────────────────────────────────────────────

/// Admin can activate emergency mode in open status
#[test]
fn test_activate_emergency_in_open_status() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );

    let result = client.activate_emergency();
    assert!(result.emergency_mode);
    assert!(client.is_emergency_mode());
}

/// Admin can activate emergency mode in funded status
#[test]
fn test_activate_emergency_in_funded_status() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);

    let result = client.activate_emergency();
    assert!(result.emergency_mode);
}

/// Cannot activate emergency mode in settled status
#[test]
#[should_panic(expected = "Cannot activate emergency mode after settlement")]
fn test_activate_emergency_after_settlement_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    client.settle();
    client.activate_emergency();
}

/// Cannot activate emergency mode twice
#[test]
#[should_panic(expected = "Emergency mode already active")]
fn test_activate_emergency_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.activate_emergency();
    client.activate_emergency();
}

// ──────────────────────────────────────────────────────────────────────────────
// Emergency Refund Tests - Happy Paths
// ──────────────────────────────────────────────────────────────────────────────

/// Single investor receives full refund in emergency mode
#[test]
fn test_emergency_refund_single_investor_full_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    client.activate_emergency();

    let refund_amount = client.emergency_refund(&investor);
    assert_eq!(refund_amount, 10_000_0000000i128);
    assert!(client.is_refunded(&investor));
}

/// Multiple investors receive proportional refunds
#[test]
fn test_emergency_refund_multiple_investors_proportional() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    
    // Investor 1 contributes 70%, Investor 2 contributes 30%
    client.fund(&investor1, &7_000_0000000i128);
    client.fund(&investor2, &3_000_0000000i128);
    client.activate_emergency();

    let refund1 = client.emergency_refund(&investor1);
    let refund2 = client.emergency_refund(&investor2);
    
    assert_eq!(refund1, 7_000_0000000i128);
    assert_eq!(refund2, 3_000_0000000i128);
    assert!(client.is_refunded(&investor1));
    assert!(client.is_refunded(&investor2));
}

/// Emergency refund works in open status (before target met)
#[test]
fn test_emergency_refund_in_open_status() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &5_000_0000000i128); // Only 50% funded
    client.activate_emergency();

    let refund_amount = client.emergency_refund(&investor);
    assert_eq!(refund_amount, 5_000_0000000i128);
}

// ──────────────────────────────────────────────────────────────────────────────
// Emergency Refund Tests - Failure Cases
// ──────────────────────────────────────────────────────────────────────────────

/// Cannot emergency refund before emergency mode is activated
#[test]
#[should_panic(expected = "Emergency mode not active")]
fn test_emergency_refund_without_activation_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    // Not calling activate_emergency()
    client.emergency_refund(&investor);
}

/// Cannot double-refund the same investor
#[test]
#[should_panic(expected = "Already refunded")]
fn test_emergency_refund_double_claim_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    client.activate_emergency();

    client.emergency_refund(&investor); // First refund succeeds
    client.emergency_refund(&investor); // Second refund fails
}

/// Cannot refund investor with zero balance
#[test]
#[should_panic(expected = "No balance to refund")]
fn test_emergency_refund_zero_balance_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    // Investor never funded
    client.activate_emergency();

    client.emergency_refund(&investor);
}

/// Cannot emergency refund after escrow is settled
#[test]
#[should_panic(expected = "Cannot activate emergency mode after settlement")]
fn test_emergency_refund_after_settlement_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    client.settle();
    // Emergency mode cannot be activated after settlement
    client.activate_emergency();
}

// ──────────────────────────────────────────────────────────────────────────────
// Emergency Refund Tests - Edge Cases
// ──────────────────────────────────────────────────────────────────────────────

/// Many investors with uneven shares
#[test]
fn test_emergency_refund_many_uneven_shares() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    let investor3 = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    
    // Uneven contributions: 1, 2, 7 units (out of 10)
    client.fund(&investor1, &1_000_0000000i128);
    client.fund(&investor2, &2_000_0000000i128);
    client.fund(&investor3, &7_000_0000000i128);
    client.activate_emergency();

    let refund1 = client.emergency_refund(&investor1);
    let refund2 = client.emergency_refund(&investor2);
    let refund3 = client.emergency_refund(&investor3);
    
    assert_eq!(refund1, 1_000_0000000i128);
    assert_eq!(refund2, 2_000_0000000i128);
    assert_eq!(refund3, 7_000_0000000i128);
}

/// Investor balance tracking across multiple fund calls
#[test]
fn test_investor_balance_accumulates() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    
    client.fund(&investor, &3_000_0000000i128);
    assert_eq!(client.get_investor_balance(&investor), 3_000_0000000i128);
    
    client.fund(&investor, &4_000_0000000i128);
    assert_eq!(client.get_investor_balance(&investor), 7_000_0000000i128);
    
    client.fund(&investor, &3_000_0000000i128);
    assert_eq!(client.get_investor_balance(&investor), 10_000_0000000i128);
}

/// Refund amount equals investor's exact balance
#[test]
fn test_emergency_refund_exact_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    client.activate_emergency();

    let refund = client.emergency_refund(&investor);
    assert_eq!(refund, client.get_investor_balance(&investor));
}

// ──────────────────────────────────────────────────────────────────────────────
// Reentrancy Protection Tests
// ──────────────────────────────────────────────────────────────────────────────

/// Verify reentrancy guard is properly cleared after operation
#[test]
fn test_reentrancy_guard_cleared_after_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor1, &5_000_0000000i128);
    client.fund(&investor2, &5_000_0000000i128);
    client.activate_emergency();

    // First refund
    client.emergency_refund(&investor1);
    
    // Second refund should work (guard was cleared)
    let refund2 = client.emergency_refund(&investor2);
    assert_eq!(refund2, 5_000_0000000i128);
}

/// Reentrancy attempt is blocked by the guard
#[test]
#[should_panic(expected = "Reentrancy detected: emergency_refund in progress")]
fn test_reentrancy_attempt_blocked() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &5_000_0000000i128);
    client.activate_emergency();

    // Simulate an in-progress refund to trigger the guard
    let contract_id = client.address.clone();
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyGuard, &true);
    });

    // This call should panic due to the guard being active
    client.emergency_refund(&investor);
}
// ──────────────────────────────────────────────────────────────────────────────
// Migration Tests
// ──────────────────────────────────────────────────────────────────────────────

/// Migration to current version fails if already at current version
#[test]
#[should_panic(expected = "Already at current schema version")]
fn test_migrate_at_current_version_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.migrate(&SCHEMA_VERSION);
}

/// Migration fails with wrong from_version
#[test]
#[should_panic(expected = "from_version does not match stored version")]
fn test_migrate_wrong_version_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.migrate(&(SCHEMA_VERSION + 1));
}

// ──────────────────────────────────────────────────────────────────────────────
// Authorization Tests
// ──────────────────────────────────────────────────────────────────────────────

/// Fund requires investor authorization
#[test]
fn test_fund_requires_investor_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &1_000_0000000i128);

    assert!(
        env.auths().iter().any(|(addr, _)| *addr == investor),
        "investor auth was not recorded for fund"
    );
}

/// Settle requires SME authorization
#[test]
fn test_settle_requires_sme_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    client.settle();

    assert!(
        env.auths().iter().any(|(addr, _)| *addr == sme),
        "sme auth was not recorded for settle"
    );
}

/// Activate emergency requires admin authorization
#[test]
fn test_activate_emergency_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.activate_emergency();

    assert!(
        env.auths().iter().any(|(addr, _)| *addr == admin),
        "admin auth was not recorded for activate_emergency"
    );
}

/// Emergency refund requires investor authorization
#[test]
fn test_emergency_refund_requires_investor_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    client.activate_emergency();
    client.emergency_refund(&investor);

    assert!(
        env.auths().iter().any(|(addr, _)| *addr == investor),
        "investor auth was not recorded for emergency_refund"
    );
}
