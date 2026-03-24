//! Unit tests for the LiquiFact escrow contract.
//!
//! Coverage targets (≥ 95 %):
//!
//! | Area                    | Tests                                                        |
//! |-------------------------|--------------------------------------------------------------|
//! | `version()`             | correct value, idempotent, type, constant sync               |
//! | `init()`                | happy-path, boundary amounts, invalid inputs                 |
//! | `init_with_admin()`     | default unpaused, admin stored correctly                     |
//! | `pause()`               | admin can pause, non-admin rejected, double-pause rejected   |
//! | `unpause()`             | admin can unpause, non-admin rejected, double-unpause        |
//! | `is_paused()`           | reflects current state, read-only at all times               |
//! | `fund()` (paused)       | blocked when paused, allowed when unpaused                   |
//! | `settle()` (paused)     | blocked when paused, allowed when unpaused                   |
//! | `get_escrow()`          | returns same reference / values                              |
//! | `fund()` (core)         | partial, exact, over-fund, status transitions                |
//! | `settle()` (core)       | happy-path, guards (pending / settled re-settle)             |
//! | Full lifecycle          | init → fund → settle end-to-end (with + without pause)       |

use crate::{Address, ContractState, Env, EscrowContract, EscrowStatus, CONTRACT_VERSION};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const ADMIN_ID: &str = "GADMINXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";
const OTHER_ID: &str = "GOTHERXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";

fn make_admin() -> Address {
    Address::from_string(ADMIN_ID)
}

fn make_other() -> Address {
    Address::from_string(OTHER_ID)
}

fn unpaused_state() -> ContractState {
    EscrowContract::init_with_admin(make_admin())
}

fn paused_state() -> ContractState {
    let mut s = unpaused_state();
    EscrowContract::pause(&mut s, &make_admin());
    s
}

fn default_escrow() -> crate::Escrow {
    EscrowContract::init(42, "GABC123".to_string(), 1_000_000, 500, 1_700_000_000)
}

// ===========================================================================
// version() tests  (Issue #26 — preserved)
// ===========================================================================

#[test]
fn test_version_matches_constant() {
    let env = Env::default();
    let v = EscrowContract::version(&env);
    assert_eq!(
        v.to_string(),
        CONTRACT_VERSION,
        "version() must return CONTRACT_VERSION"
    );
}

#[test]
fn test_version_is_semver_format() {
    let env = Env::default();
    let v = EscrowContract::version(&env).to_string();
    let parts: Vec<&str> = v.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "version must have three dot-separated segments"
    );
    for part in &parts {
        part.parse::<u32>()
            .expect("each version segment must be a non-negative integer");
    }
}

#[test]
fn test_version_is_idempotent() {
    let env = Env::default();
    let v1 = EscrowContract::version(&env).to_string();
    let v2 = EscrowContract::version(&env).to_string();
    assert_eq!(v1, v2, "version() must be pure and idempotent");
}

#[test]
fn test_version_major_is_one() {
    let env = Env::default();
    let v = EscrowContract::version(&env).to_string();
    assert!(
        v.starts_with("1."),
        "initial release must have MAJOR = 1, got: {v}"
    );
}

#[test]
fn test_contract_version_constant_not_empty() {
    assert!(
        !CONTRACT_VERSION.is_empty(),
        "CONTRACT_VERSION must not be empty"
    );
}

#[test]
fn test_version_not_zero() {
    let env = Env::default();
    let v = EscrowContract::version(&env).to_string();
    assert_ne!(v, "0.0.0", "version must not be the zero sentinel");
}

#[test]
fn test_version_soroban_string_roundtrip() {
    let env = Env::default();
    let soroban_str = EscrowContract::version(&env);
    let rust_str = soroban_str.to_string();
    let rewrapped = crate::SorobanString::from_str(&env, &rust_str);
    assert_eq!(soroban_str, rewrapped);
}

// ===========================================================================
// init_with_admin() tests  (Issue #24)
// ===========================================================================

/// Fresh governance state must default to unpaused.
#[test]
fn test_init_with_admin_starts_unpaused() {
    let state = unpaused_state();
    assert!(!state.paused, "contract must start in unpaused state");
}

/// Admin address must be stored exactly as supplied.
#[test]
fn test_init_with_admin_stores_admin_address() {
    let state = unpaused_state();
    assert_eq!(
        state.admin,
        make_admin(),
        "stored admin must match the address passed to init_with_admin"
    );
}

/// Two separate ContractState instances are independent.
#[test]
fn test_init_with_admin_independent_states() {
    let state_a = EscrowContract::init_with_admin(Address::from_string("GADMIN_A"));
    let state_b = EscrowContract::init_with_admin(Address::from_string("GADMIN_B"));
    assert_ne!(state_a.admin, state_b.admin);
    assert!(!state_a.paused);
    assert!(!state_b.paused);
}

// ===========================================================================
// pause() tests  (Issue #24)
// ===========================================================================

/// Admin can pause an unpaused contract.
#[test]
fn test_pause_by_admin_succeeds() {
    let mut state = unpaused_state();
    EscrowContract::pause(&mut state, &make_admin());
    assert!(state.paused, "contract must be paused after pause()");
}

/// is_paused() returns true immediately after pause.
#[test]
fn test_pause_is_reflected_in_is_paused() {
    let mut state = unpaused_state();
    EscrowContract::pause(&mut state, &make_admin());
    assert!(EscrowContract::is_paused(&state));
}

/// Non-admin caller must be rejected.
#[test]
#[should_panic(expected = "caller is not admin")]
fn test_pause_by_non_admin_panics() {
    let mut state = unpaused_state();
    EscrowContract::pause(&mut state, &make_other());
}

/// Pausing an already-paused contract must be rejected.
#[test]
#[should_panic(expected = "contract already paused")]
fn test_pause_when_already_paused_panics() {
    let mut state = paused_state();
    EscrowContract::pause(&mut state, &make_admin());
}

/// Non-admin on already-paused: non-admin check fires first.
#[test]
#[should_panic(expected = "caller is not admin")]
fn test_pause_non_admin_on_paused_contract_panics() {
    let mut state = paused_state();
    EscrowContract::pause(&mut state, &make_other());
}

// ===========================================================================
// unpause() tests  (Issue #24)
// ===========================================================================

/// Admin can unpause a paused contract.
#[test]
fn test_unpause_by_admin_succeeds() {
    let mut state = paused_state();
    EscrowContract::unpause(&mut state, &make_admin());
    assert!(!state.paused, "contract must be unpaused after unpause()");
}

/// is_paused() returns false after unpause.
#[test]
fn test_unpause_is_reflected_in_is_paused() {
    let mut state = paused_state();
    EscrowContract::unpause(&mut state, &make_admin());
    assert!(!EscrowContract::is_paused(&state));
}

/// Non-admin caller must be rejected.
#[test]
#[should_panic(expected = "caller is not admin")]
fn test_unpause_by_non_admin_panics() {
    let mut state = paused_state();
    EscrowContract::unpause(&mut state, &make_other());
}

/// Unpausing an already-unpaused contract must be rejected.
#[test]
#[should_panic(expected = "contract not paused")]
fn test_unpause_when_not_paused_panics() {
    let mut state = unpaused_state();
    EscrowContract::unpause(&mut state, &make_admin());
}

// ===========================================================================
// is_paused() tests  (Issue #24)
// ===========================================================================

#[test]
fn test_is_paused_false_initially() {
    let state = unpaused_state();
    assert!(!EscrowContract::is_paused(&state));
}

#[test]
fn test_is_paused_true_after_pause() {
    let state = paused_state();
    assert!(EscrowContract::is_paused(&state));
}

#[test]
fn test_is_paused_false_after_unpause() {
    let mut state = paused_state();
    EscrowContract::unpause(&mut state, &make_admin());
    assert!(!EscrowContract::is_paused(&state));
}

#[test]
fn test_is_paused_does_not_mutate() {
    let state = paused_state();
    let _ = EscrowContract::is_paused(&state);
    let _ = EscrowContract::is_paused(&state);
    assert!(state.paused);
}

// ===========================================================================
// fund() — pause guard tests  (Issue #24)
// ===========================================================================

#[test]
#[should_panic(expected = "contract is paused")]
fn test_fund_blocked_when_paused() {
    let state = paused_state();
    let mut escrow = default_escrow();
    EscrowContract::fund(&state, &mut escrow, 100_000);
}

#[test]
fn test_fund_allowed_when_unpaused() {
    let state = unpaused_state();
    let mut escrow = default_escrow();
    EscrowContract::fund(&state, &mut escrow, 500_000);
    assert_eq!(escrow.funded_amount, 500_000);
}

#[test]
fn test_fund_allowed_after_unpause() {
    let mut state = unpaused_state();
    let mut escrow = default_escrow();
    EscrowContract::pause(&mut state, &make_admin());
    EscrowContract::unpause(&mut state, &make_admin());
    EscrowContract::fund(&state, &mut escrow, 500_000);
    assert_eq!(escrow.funded_amount, 500_000);
}

/// Pause check fires before amount validation.
#[test]
#[should_panic(expected = "contract is paused")]
fn test_fund_paused_check_before_amount_check() {
    let state = paused_state();
    let mut escrow = default_escrow();
    EscrowContract::fund(&state, &mut escrow, 0);
}

// ===========================================================================
// settle() — pause guard tests  (Issue #24)
// ===========================================================================

#[test]
#[should_panic(expected = "contract is paused")]
fn test_settle_blocked_when_paused() {
    let mut state = unpaused_state();
    let mut escrow = default_escrow();
    EscrowContract::fund(&state, &mut escrow, 1_000_000);
    EscrowContract::pause(&mut state, &make_admin());
    EscrowContract::settle(&state, &mut escrow);
}

#[test]
fn test_settle_allowed_when_unpaused() {
    let state = unpaused_state();
    let mut escrow = default_escrow();
    EscrowContract::fund(&state, &mut escrow, 1_000_000);
    EscrowContract::settle(&state, &mut escrow);
    assert_eq!(escrow.status, EscrowStatus::Settled);
}

#[test]
fn test_settle_allowed_after_unpause() {
    let mut state = unpaused_state();
    let mut escrow = default_escrow();
    EscrowContract::fund(&state, &mut escrow, 1_000_000);
    EscrowContract::pause(&mut state, &make_admin());
    EscrowContract::unpause(&mut state, &make_admin());
    EscrowContract::settle(&state, &mut escrow);
    assert_eq!(escrow.status, EscrowStatus::Settled);
}

/// Pause check fires before escrow-status validation.
#[test]
#[should_panic(expected = "contract is paused")]
fn test_settle_paused_check_before_status_check() {
    let state = paused_state();
    let mut escrow = default_escrow(); // Pending — would normally fail "must be funded"
    EscrowContract::settle(&state, &mut escrow);
}

// ===========================================================================
// init() tests  (Issue #26 — preserved)
// ===========================================================================

#[test]
fn test_init_happy_path() {
    let e = default_escrow();
    assert_eq!(e.invoice_id, 42);
    assert_eq!(e.sme_address, "GABC123");
    assert_eq!(e.amount, 1_000_000);
    assert_eq!(e.yield_bps, 500);
    assert_eq!(e.maturity, 1_700_000_000);
    assert_eq!(e.funded_amount, 0);
    assert_eq!(e.status, EscrowStatus::Pending);
}

#[test]
fn test_init_minimum_amount() {
    let e = EscrowContract::init(1, "GSME".to_string(), 1, 0, 0);
    assert_eq!(e.amount, 1);
}

#[test]
fn test_init_zero_yield_bps() {
    let e = EscrowContract::init(1, "GSME".to_string(), 100, 0, 0);
    assert_eq!(e.yield_bps, 0);
}

#[test]
fn test_init_max_yield_bps() {
    let e = EscrowContract::init(1, "GSME".to_string(), 100, 10_000, 0);
    assert_eq!(e.yield_bps, 10_000);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_init_zero_amount_panics() {
    EscrowContract::init(1, "GSME".to_string(), 0, 0, 0);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_init_negative_amount_panics() {
    EscrowContract::init(1, "GSME".to_string(), -1, 0, 0);
}

#[test]
#[should_panic(expected = "yield_bps must be <= 10000")]
fn test_init_yield_bps_overflow_panics() {
    EscrowContract::init(1, "GSME".to_string(), 100, 10_001, 0);
}

// ===========================================================================
// get_escrow() tests  (Issue #26 — preserved)
// ===========================================================================

#[test]
fn test_get_escrow_returns_correct_state() {
    let e = default_escrow();
    let read = EscrowContract::get_escrow(&e);
    assert_eq!(read.invoice_id, e.invoice_id);
    assert_eq!(read.amount, e.amount);
    assert_eq!(read.status, EscrowStatus::Pending);
}

// ===========================================================================
// fund() — core tests  (Issue #26 — preserved, updated signature)
// ===========================================================================

#[test]
fn test_fund_partial_stays_pending() {
    let state = unpaused_state();
    let mut e = default_escrow();
    EscrowContract::fund(&state, &mut e, 500_000);
    assert_eq!(e.funded_amount, 500_000);
    assert_eq!(e.status, EscrowStatus::Pending);
}

#[test]
fn test_fund_exact_becomes_funded() {
    let state = unpaused_state();
    let mut e = default_escrow();
    EscrowContract::fund(&state, &mut e, 1_000_000);
    assert_eq!(e.funded_amount, 1_000_000);
    assert_eq!(e.status, EscrowStatus::Funded);
}

#[test]
fn test_fund_over_amount_becomes_funded() {
    let state = unpaused_state();
    let mut e = default_escrow();
    EscrowContract::fund(&state, &mut e, 1_500_000);
    assert_eq!(e.funded_amount, 1_500_000);
    assert_eq!(e.status, EscrowStatus::Funded);
}

#[test]
fn test_fund_multiple_tranches() {
    let state = unpaused_state();
    let mut e = default_escrow();
    EscrowContract::fund(&state, &mut e, 300_000);
    EscrowContract::fund(&state, &mut e, 300_000);
    EscrowContract::fund(&state, &mut e, 400_000);
    assert_eq!(e.funded_amount, 1_000_000);
    assert_eq!(e.status, EscrowStatus::Funded);
}

#[test]
#[should_panic(expected = "fund_amount must be positive")]
fn test_fund_zero_panics() {
    let state = unpaused_state();
    let mut e = default_escrow();
    EscrowContract::fund(&state, &mut e, 0);
}

#[test]
#[should_panic(expected = "fund_amount must be positive")]
fn test_fund_negative_panics() {
    let state = unpaused_state();
    let mut e = default_escrow();
    EscrowContract::fund(&state, &mut e, -1);
}

#[test]
#[should_panic(expected = "cannot fund a settled escrow")]
fn test_fund_settled_escrow_panics() {
    let state = unpaused_state();
    let mut e = default_escrow();
    EscrowContract::fund(&state, &mut e, 1_000_000);
    EscrowContract::settle(&state, &mut e);
    EscrowContract::fund(&state, &mut e, 1);
}

// ===========================================================================
// settle() — core tests  (Issue #26 — preserved, updated signature)
// ===========================================================================

#[test]
fn test_settle_happy_path() {
    let state = unpaused_state();
    let mut e = default_escrow();
    EscrowContract::fund(&state, &mut e, 1_000_000);
    EscrowContract::settle(&state, &mut e);
    assert_eq!(e.status, EscrowStatus::Settled);
}

#[test]
#[should_panic(expected = "escrow must be funded before settlement")]
fn test_settle_pending_escrow_panics() {
    let state = unpaused_state();
    let mut e = default_escrow();
    EscrowContract::settle(&state, &mut e);
}

#[test]
#[should_panic(expected = "escrow must be funded before settlement")]
fn test_settle_already_settled_panics() {
    let state = unpaused_state();
    let mut e = default_escrow();
    EscrowContract::fund(&state, &mut e, 1_000_000);
    EscrowContract::settle(&state, &mut e);
    EscrowContract::settle(&state, &mut e);
}

// ===========================================================================
// Full lifecycle integration tests
// ===========================================================================

/// End-to-end happy path: no pause involved.
#[test]
fn test_full_lifecycle_unpaused() {
    let env = Env::default();
    let state = unpaused_state();

    let v = EscrowContract::version(&env).to_string();
    assert_eq!(v, "1.1.0");

    let mut escrow = EscrowContract::init(
        99,
        "GSME_FULL_LIFECYCLE".to_string(),
        2_000_000,
        300,
        1_800_000_000,
    );

    EscrowContract::fund(&state, &mut escrow, 999_999);
    assert_eq!(escrow.status, EscrowStatus::Pending);

    EscrowContract::fund(&state, &mut escrow, 1_000_001);
    assert_eq!(escrow.status, EscrowStatus::Funded);
    assert_eq!(escrow.funded_amount, 2_000_000);

    EscrowContract::settle(&state, &mut escrow);
    assert_eq!(escrow.status, EscrowStatus::Settled);

    let read = EscrowContract::get_escrow(&escrow);
    assert_eq!(read.invoice_id, 99);
    assert_eq!(read.yield_bps, 300);
}

/// Pause mid-lifecycle: fund halted, resumed, then settled.
#[test]
fn test_full_lifecycle_with_pause_and_resume() {
    let mut state = unpaused_state();
    let mut escrow = EscrowContract::init(
        77,
        "GSME_PAUSE_RESUME".to_string(),
        1_000_000,
        200,
        1_900_000_000,
    );

    EscrowContract::fund(&state, &mut escrow, 400_000);
    assert_eq!(escrow.funded_amount, 400_000);

    // Incident: pause.
    EscrowContract::pause(&mut state, &make_admin());
    assert!(EscrowContract::is_paused(&state));

    // Incident resolved: unpause.
    EscrowContract::unpause(&mut state, &make_admin());
    assert!(!EscrowContract::is_paused(&state));

    EscrowContract::fund(&state, &mut escrow, 600_000);
    assert_eq!(escrow.status, EscrowStatus::Funded);

    EscrowContract::settle(&state, &mut escrow);
    assert_eq!(escrow.status, EscrowStatus::Settled);
}

/// Multiple pause/unpause cycles must not corrupt state.
#[test]
fn test_multiple_pause_unpause_cycles() {
    let mut state = unpaused_state();
    for _ in 0..3 {
        EscrowContract::pause(&mut state, &make_admin());
        assert!(state.paused);
        EscrowContract::unpause(&mut state, &make_admin());
        assert!(!state.paused);
    }
}

/// Read-only methods are never blocked by pause state.
#[test]
fn test_read_only_methods_unaffected_by_pause() {
    let env = Env::default();
    let state = paused_state();

    let v = EscrowContract::version(&env).to_string();
    assert!(!v.is_empty());

    let paused = EscrowContract::is_paused(&state);
    assert!(paused);

    let escrow = default_escrow();
    let read = EscrowContract::get_escrow(&escrow);
    assert_eq!(read.invoice_id, 42);
}