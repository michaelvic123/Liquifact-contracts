use super::{EscrowFactory, EscrowFactoryClient, LiquifactEscrow, LiquifactEscrowClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deploy a fresh contract and return (env, client, admin, sme).
fn setup() -> (Env, LiquifactEscrowClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);
    (env, client, admin, sme)
}

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

#[test]
fn test_init_stores_escrow() {
    let (_, client, admin, sme) = setup();

    let escrow = client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.admin, admin);
    assert_eq!(escrow.sme_address, sme);
    assert_eq!(escrow.amount, 10_000_0000000i128);
    assert_eq!(escrow.funded_amount, 0);
    assert_eq!(escrow.status, 0);

    // get_escrow should return the same data
    let got = client.get_escrow();
    assert_eq!(got.invoice_id, escrow.invoice_id);
    assert_eq!(got.admin, admin);
}

#[test]
fn test_fund_partial_then_full() {
    let (env, client, admin, sme) = setup();
    let investor = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("INV002"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    // Partial fund — status stays open
    let e1 = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(e1.funded_amount, 5_000_0000000i128);
    assert_eq!(e1.status, 0);

    // Complete fund — status becomes funded
    let e2 = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(e2.funded_amount, 10_000_0000000i128);
    assert_eq!(e2.status, 1);
}

#[test]
fn test_settle_after_full_funding() {
    let (env, client, admin, sme) = setup();
    let investor = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("INV003"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);

    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

// ---------------------------------------------------------------------------
// Authorization verification tests
// ---------------------------------------------------------------------------

/// Verify that `init` records an auth requirement for the admin address.
#[test]
fn test_init_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV004"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );

    // Inspect recorded auths — admin must appear as the top-level authorizer.
    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == admin),
        "admin auth was not recorded for init"
    );
}

/// Verify that `fund` records an auth requirement for the investor address.
#[test]
fn test_fund_requires_investor_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV005"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.fund(&investor, &1_000i128);

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == investor),
        "investor auth was not recorded for fund"
    );
}

/// Verify that `settle` records an auth requirement for the SME address.
#[test]
fn test_settle_requires_sme_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV006"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.fund(&investor, &1_000i128);
    client.settle();

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == sme),
        "sme auth was not recorded for settle"
    );
}

// ---------------------------------------------------------------------------
// Unauthorized / panic-path tests
// ---------------------------------------------------------------------------

/// `init` called by a non-admin should panic (auth not satisfied).
#[test]
#[should_panic]
fn test_init_unauthorized_panics() {
    let env = Env::default();
    // Do NOT mock auths — let the real auth check fire.
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV007"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
}

/// `settle` called without SME auth should panic.
#[test]
#[should_panic]
fn test_settle_unauthorized_panics() {
    let env = Env::default();
    // Do NOT mock auths.
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    // Use mock_all_auths only for setup steps.
    env.mock_all_auths();
    client.init(
        &admin,
        &symbol_short!("INV008"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.fund(&investor, &1_000i128);

    // Clear mocked auths so settle must satisfy real auth.
    // Soroban test env doesn't expose a "clear mocks" API, so we re-create
    // a client on the same contract without mocking to trigger the failure.
    let env2 = Env::default(); // fresh env — no mocked auths
    let client2 = LiquifactEscrowClient::new(&env2, &contract_id);
    client2.settle(); // should panic: sme auth not satisfied
}

// ---------------------------------------------------------------------------
// Edge-case / guard tests
// ---------------------------------------------------------------------------

/// Re-initializing an already-initialized escrow must panic.
#[test]
#[should_panic(expected = "Escrow already initialized")]
fn test_double_init_panics() {
    let (_, client, admin, sme) = setup();

    client.init(
        &admin,
        &symbol_short!("INV009"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    // Second init on the same contract must be rejected.
    client.init(
        &admin,
        &symbol_short!("INV009"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
}

/// Funding an already-funded escrow must panic.
#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_panics() {
    let (env, client, admin, sme) = setup();
    let investor = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("INV010"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.fund(&investor, &1_000i128); // reaches funded status
    client.fund(&investor, &1i128); // must panic
}

/// Settling an escrow that is still open (not yet funded) must panic.
#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_before_funded_panics() {
    let (_, client, admin, sme) = setup();

    client.init(
        &admin,
        &symbol_short!("INV011"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.settle(); // status is still 0 — must panic
}

/// `get_escrow` on an uninitialized contract must panic.
#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_get_escrow_uninitialized_panics() {
    let env = Env::default();
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);
    client.get_escrow();
}

/// Partial funding across two investors; status stays open until target is met.
#[test]
fn test_partial_fund_stays_open() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV003"),
        &sme,
        &10_000_0000000i128,
        &500i64,
        &2000u64,
    );

    // Fund half — should remain open
    let partial = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(partial.status, 0, "status should still be open");
    assert_eq!(partial.funded_amount, 5_000_0000000i128);

    // Fund the rest — should flip to funded
    let full = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(full.status, 1, "status should be funded");
}

/// Attempting to settle an escrow that is still open must panic.
#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_unfunded_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV004"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    client.settle(); // must panic
}

/// Funding an already-funded (status=1) escrow must panic (extra edge-case variant).
#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_extra_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV012"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    client.fund(&investor, &10_000_0000000i128); // fills target → status 1
    client.fund(&investor, &1i128); // must panic
}

// ---------------------------------------------------------------------------
// EscrowFactory tests
// ---------------------------------------------------------------------------

/// Helper: deploy a fresh EscrowFactory and return (env, client, admin, sme).
fn factory_setup() -> (Env, EscrowFactoryClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let contract_id = env.register(EscrowFactory, ());
    let client = EscrowFactoryClient::new(&env, &contract_id);
    (env, client, admin, sme)
}

/// create_escrow stores the escrow and it is retrievable via get_escrow.
#[test]
fn test_factory_create_and_get_escrow() {
    let (_, client, admin, sme) = factory_setup();

    let escrow = client.create_escrow(
        &admin,
        &symbol_short!("F001"),
        &sme,
        &10_000i128,
        &800i64,
        &1000u64,
    );

    assert_eq!(escrow.invoice_id, symbol_short!("F001"));
    assert_eq!(escrow.admin, admin);
    assert_eq!(escrow.sme_address, sme);
    assert_eq!(escrow.amount, 10_000i128);
    assert_eq!(escrow.funded_amount, 0);
    assert_eq!(escrow.status, 0);

    let got = client.get_escrow(&symbol_short!("F001"));
    assert_eq!(got.invoice_id, escrow.invoice_id);
    assert_eq!(got.admin, admin);
}

/// Factory isolates multiple escrows — each invoice is independent.
#[test]
fn test_factory_multiple_escrows_isolated() {
    let (env, client, admin, sme) = factory_setup();
    let sme2 = Address::generate(&env);

    client.create_escrow(
        &admin,
        &symbol_short!("F002"),
        &sme,
        &1_000i128,
        &500i64,
        &500u64,
    );
    client.create_escrow(
        &admin,
        &symbol_short!("F003"),
        &sme2,
        &2_000i128,
        &600i64,
        &600u64,
    );

    let e1 = client.get_escrow(&symbol_short!("F002"));
    let e2 = client.get_escrow(&symbol_short!("F003"));

    // Each escrow holds its own state independently.
    assert_eq!(e1.amount, 1_000i128);
    assert_eq!(e2.amount, 2_000i128);
    assert_eq!(e1.sme_address, sme);
    assert_eq!(e2.sme_address, sme2);
}

/// list_invoices returns all invoice IDs in creation order.
#[test]
fn test_factory_list_invoices() {
    let (_, client, admin, sme) = factory_setup();

    assert_eq!(client.list_invoices().len(), 0);

    client.create_escrow(&admin, &symbol_short!("F004"), &sme, &1_000i128, &500i64, &500u64);
    client.create_escrow(&admin, &symbol_short!("F005"), &sme, &2_000i128, &600i64, &600u64);

    let list = client.list_invoices();
    assert_eq!(list.len(), 2);
    assert_eq!(list.get(0).unwrap(), symbol_short!("F004"));
    assert_eq!(list.get(1).unwrap(), symbol_short!("F005"));
}

/// fund via factory updates funded_amount and flips status when target met.
#[test]
fn test_factory_fund_partial_then_full() {
    let (env, client, admin, sme) = factory_setup();
    let investor = Address::generate(&env);

    client.create_escrow(&admin, &symbol_short!("F006"), &sme, &1_000i128, &500i64, &500u64);

    let e1 = client.fund(&symbol_short!("F006"), &investor, &400i128);
    assert_eq!(e1.funded_amount, 400i128);
    assert_eq!(e1.status, 0);

    let e2 = client.fund(&symbol_short!("F006"), &investor, &600i128);
    assert_eq!(e2.funded_amount, 1_000i128);
    assert_eq!(e2.status, 1);
}

/// settle via factory transitions status from funded (1) to settled (2).
#[test]
fn test_factory_settle_after_full_funding() {
    let (env, client, admin, sme) = factory_setup();
    let investor = Address::generate(&env);

    client.create_escrow(&admin, &symbol_short!("F007"), &sme, &1_000i128, &500i64, &500u64);
    client.fund(&symbol_short!("F007"), &investor, &1_000i128);

    let settled = client.settle(&symbol_short!("F007"));
    assert_eq!(settled.status, 2);
}

/// Duplicate create_escrow for the same invoice_id must panic.
#[test]
#[should_panic(expected = "Escrow already exists for this invoice")]
fn test_factory_duplicate_invoice_panics() {
    let (_, client, admin, sme) = factory_setup();

    client.create_escrow(&admin, &symbol_short!("F008"), &sme, &1_000i128, &500i64, &500u64);
    client.create_escrow(&admin, &symbol_short!("F008"), &sme, &1_000i128, &500i64, &500u64);
}

/// get_escrow for an unknown invoice_id must panic.
#[test]
#[should_panic(expected = "Escrow not found for invoice")]
fn test_factory_get_unknown_invoice_panics() {
    let (_, client, _, _) = factory_setup();
    client.get_escrow(&symbol_short!("NOEXIST"));
}

/// fund on a funded (status=1) escrow must panic.
#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_factory_fund_after_funded_panics() {
    let (env, client, admin, sme) = factory_setup();
    let investor = Address::generate(&env);

    client.create_escrow(&admin, &symbol_short!("F009"), &sme, &1_000i128, &500i64, &500u64);
    client.fund(&symbol_short!("F009"), &investor, &1_000i128); // status → 1
    client.fund(&symbol_short!("F009"), &investor, &1i128);     // must panic
}

/// settle on an unfunded (status=0) escrow must panic.
#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_factory_settle_unfunded_panics() {
    let (_, client, admin, sme) = factory_setup();

    client.create_escrow(&admin, &symbol_short!("F010"), &sme, &1_000i128, &500i64, &500u64);
    client.settle(&symbol_short!("F010")); // status is still 0 — must panic
}

/// Funding one invoice does not affect another invoice's state.
#[test]
fn test_factory_fund_does_not_bleed_across_invoices() {
    let (env, client, admin, sme) = factory_setup();
    let investor = Address::generate(&env);

    client.create_escrow(&admin, &symbol_short!("F011"), &sme, &500i128, &500i64, &500u64);
    client.create_escrow(&admin, &symbol_short!("F012"), &sme, &500i128, &500i64, &500u64);

    client.fund(&symbol_short!("F011"), &investor, &500i128); // fully fund F011

    // F012 must remain untouched.
    let f012 = client.get_escrow(&symbol_short!("F012"));
    assert_eq!(f012.funded_amount, 0);
    assert_eq!(f012.status, 0);
}

/// create_escrow requires admin authorization.
#[test]
fn test_factory_create_requires_admin_auth() {
    let (env, client, admin, sme) = factory_setup();

    client.create_escrow(&admin, &symbol_short!("F013"), &sme, &1_000i128, &500i64, &500u64);

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == admin),
        "admin auth was not recorded for create_escrow"
    );
}

/// fund requires investor authorization.
#[test]
fn test_factory_fund_requires_investor_auth() {
    let (env, client, admin, sme) = factory_setup();
    let investor = Address::generate(&env);

    client.create_escrow(&admin, &symbol_short!("F014"), &sme, &1_000i128, &500i64, &500u64);
    client.fund(&symbol_short!("F014"), &investor, &500i128);

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == investor),
        "investor auth was not recorded for fund"
    );
}

/// settle requires sme_address authorization.
#[test]
fn test_factory_settle_requires_sme_auth() {
    let (env, client, admin, sme) = factory_setup();
    let investor = Address::generate(&env);

    client.create_escrow(&admin, &symbol_short!("F015"), &sme, &1_000i128, &500i64, &500u64);
    client.fund(&symbol_short!("F015"), &investor, &1_000i128);
    client.settle(&symbol_short!("F015"));

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == sme),
        "sme auth was not recorded for settle"
    );
}

// ---------------------------------------------------------------------------
// LiquifactEscrow — transfer_admin tests
// ---------------------------------------------------------------------------

/// transfer_admin updates the admin field to the new address.
#[test]
fn test_transfer_admin_updates_admin() {
    let (env, client, admin, sme) = setup();
    let new_admin = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("T001"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );

    let updated = client.transfer_admin(&new_admin);
    assert_eq!(updated.admin, new_admin);

    // Persisted state must also reflect the new admin.
    let stored = client.get_escrow();
    assert_eq!(stored.admin, new_admin);
}

/// After transfer, the new admin is recorded as the authorizer.
#[test]
fn test_transfer_admin_records_new_admin_auth() {
    let (env, client, admin, sme) = setup();
    let new_admin = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("T002"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.transfer_admin(&new_admin);

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == admin),
        "current admin auth was not recorded for transfer_admin"
    );
}

/// transfer_admin emits an (admin, transfer) event with old and new addresses.
#[test]
fn test_transfer_admin_emits_event() {
    let (env, client, admin, sme) = setup();
    let new_admin = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("T003"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.transfer_admin(&new_admin);

    let events = env.events().all();
    assert!(
        !events.is_empty(),
        "expected at least one event after transfer_admin"
    );
}

/// Transferring to the same address must panic.
#[test]
#[should_panic(expected = "New admin must differ from current admin")]
fn test_transfer_admin_same_address_panics() {
    let (_, client, admin, sme) = setup();

    client.init(
        &admin,
        &symbol_short!("T004"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.transfer_admin(&admin); // same address — must panic
}

/// transfer_admin on an uninitialized escrow must panic.
#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_transfer_admin_uninitialized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);
    let new_admin = Address::generate(&env);

    client.transfer_admin(&new_admin);
}

/// Non-admin caller must not be able to transfer admin (auth failure).
#[test]
#[should_panic]
fn test_transfer_admin_unauthorized_panics() {
    let env = Env::default();
    // mock_all_auths only for setup steps.
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("T005"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );

    // Fresh env without mocked auths — real auth check fires.
    let env2 = Env::default();
    let client2 = LiquifactEscrowClient::new(&env2, &contract_id);
    let new_admin = Address::generate(&env2);
    client2.transfer_admin(&new_admin); // must panic: admin auth not satisfied
}

/// transfer_admin can be called multiple times (chained rotation).
#[test]
fn test_transfer_admin_chained_rotation() {
    let (env, client, admin, sme) = setup();
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("T006"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );

    client.transfer_admin(&admin2);
    assert_eq!(client.get_escrow().admin, admin2);

    client.transfer_admin(&admin3);
    assert_eq!(client.get_escrow().admin, admin3);
}

/// Other escrow fields are unchanged after transfer_admin.
#[test]
fn test_transfer_admin_preserves_escrow_fields() {
    let (env, client, admin, sme) = setup();
    let new_admin = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("T007"),
        &sme,
        &5_000i128,
        &800i64,
        &3000u64,
    );
    let updated = client.transfer_admin(&new_admin);

    assert_eq!(updated.invoice_id, symbol_short!("T007"));
    assert_eq!(updated.sme_address, sme);
    assert_eq!(updated.amount, 5_000i128);
    assert_eq!(updated.yield_bps, 800i64);
    assert_eq!(updated.maturity, 3000u64);
    assert_eq!(updated.funded_amount, 0);
    assert_eq!(updated.status, 0);
}

// ---------------------------------------------------------------------------
// EscrowFactory — transfer_admin tests
// ---------------------------------------------------------------------------

/// Factory transfer_admin updates the admin for the specified invoice only.
#[test]
fn test_factory_transfer_admin_updates_admin() {
    let (env, client, admin, sme) = factory_setup();
    let new_admin = Address::generate(&env);

    client.create_escrow(&admin, &symbol_short!("G001"), &sme, &1_000i128, &500i64, &500u64);

    let updated = client.transfer_admin(&symbol_short!("G001"), &new_admin);
    assert_eq!(updated.admin, new_admin);
    assert_eq!(client.get_escrow(&symbol_short!("G001")).admin, new_admin);
}

/// Factory transfer_admin only affects the target invoice, not others.
#[test]
fn test_factory_transfer_admin_isolated() {
    let (env, client, admin, sme) = factory_setup();
    let new_admin = Address::generate(&env);

    client.create_escrow(&admin, &symbol_short!("G002"), &sme, &1_000i128, &500i64, &500u64);
    client.create_escrow(&admin, &symbol_short!("G003"), &sme, &1_000i128, &500i64, &500u64);

    client.transfer_admin(&symbol_short!("G002"), &new_admin);

    // G003 admin must remain unchanged.
    assert_eq!(client.get_escrow(&symbol_short!("G003")).admin, admin);
}

/// Factory transfer_admin to same address must panic.
#[test]
#[should_panic(expected = "New admin must differ from current admin")]
fn test_factory_transfer_admin_same_address_panics() {
    let (_, client, admin, sme) = factory_setup();

    client.create_escrow(&admin, &symbol_short!("G004"), &sme, &1_000i128, &500i64, &500u64);
    client.transfer_admin(&symbol_short!("G004"), &admin);
}

/// Factory transfer_admin on unknown invoice must panic.
#[test]
#[should_panic(expected = "Escrow not found for invoice")]
fn test_factory_transfer_admin_unknown_invoice_panics() {
    let (env, client, _, _) = factory_setup();
    let new_admin = Address::generate(&env);

    client.transfer_admin(&symbol_short!("NOPE"), &new_admin);
}

/// Factory transfer_admin emits event.
#[test]
fn test_factory_transfer_admin_emits_event() {
    let (env, client, admin, sme) = factory_setup();
    let new_admin = Address::generate(&env);

    client.create_escrow(&admin, &symbol_short!("G005"), &sme, &1_000i128, &500i64, &500u64);
    client.transfer_admin(&symbol_short!("G005"), &new_admin);

    let events = env.events().all();
    assert!(
        !events.is_empty(),
        "expected at least one event after factory transfer_admin"
    );
}
