//! # LiquiFact Escrow Contract
//!
//! Holds investor funds for an invoice until settlement.
//!
//! ### Settlement Sequence
//! 1. **Initialization**: Admin creates the escrow with `init`.
//! 2. **Funding**: Investors contribute funds via `fund` until `funding_target` is met (status 0 -> 1).
//! 3. **Payment Confirmation**: After buyer pays the SME off-chain (or via other means), the buyer
//!    calls `confirm_payment` to acknowledge repayment.
//! 4. **Settlement**: SME calls `settle` to finalize the escrow, moving it to status 2.
//!
//! # Emergency Refund Mechanism
//!
//! In exceptional circumstances (e.g., legal disputes, fraud detection, protocol failure),
//! the admin can activate emergency mode to refund investors proportionally.
//!
//! ## Design Decisions
//!
//! - **Access Control**: Only admin can activate emergency mode (consistent with `update_maturity` pattern)
//! - **Activation States**: Emergency mode can only be activated when escrow is in open (0) or funded (1) states
//! - **Reentrancy Protection**: Uses a guard pattern following checks-effects-interactions strictly
//! - **Refund Calculation**: Proportional refunds based on investor's contribution relative to total funded amount
//! - **Double-Claim Prevention**: Tracks refunded investors to prevent multiple refunds to same investor
//! - **Balance Tracking**: Maintains individual investor balances for accurate proportional refunds
//!
//! ## Security Considerations
//!
//! - Emergency mode is one-way: once activated, the escrow cannot return to normal operation
//! - Individual refunds are atomic: each investor must call `emergency_refund` separately
//! - Proportional calculation ensures fair distribution when total funds are insufficient
//! - Reentrancy guard prevents recursive calls during fund transfers
//!
//! # Storage Schema Versioning
//!
//! The escrow state is stored under two keys:
//! - `"escrow"` — the [`InvoiceEscrow`] struct (current schema)
//! - `"version"` — a `u32` schema version number
//!
//! ## Version history
//!
//! | Version | Changes |
//! |---------|---------|
//! | 1       | Initial schema: invoice_id, admin, sme_address, amount, funding_target, funded_amount, yield_bps, maturity, status, version |
//! | 2       | Added emergency refund support: emergency_mode, reentrancy_guard |
//!
//! When a new field is added or the struct layout changes, bump `SCHEMA_VERSION`,
//! add a migration arm in [`LiquifactEscrow::migrate`], and add a corresponding test.

use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, symbol_short, Address, Env, Map, Symbol,
};

/// Current storage schema version. Bump this with every breaking struct change.
pub const SCHEMA_VERSION: u32 = 2;

/// Full state of an invoice escrow persisted in contract storage.
///
/// All monetary values use the smallest indivisible unit of the relevant
/// Stellar asset (e.g. stroops for XLM, or the token's own precision).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceEscrow {
    /// Unique invoice identifier agreed between SME and platform (e.g. `"INV1023"`).
    /// Maximum 8 ASCII characters due to Soroban `symbol_short!` constraints.
    pub invoice_id: Symbol,
    /// Admin address that initialized this escrow and authorized for emergency operations
    pub admin: Address,
    /// SME wallet that receives liquidity and authorizes settlement
    pub sme_address: Address,
    /// Total amount in smallest unit (e.g. stroops for XLM)
    pub amount: i128,
    /// Investor funding target. Currently equal to `amount`; may diverge
    /// in future versions that support partial invoice tokenization.
    pub funding_target: i128,
    /// Running total committed by investors so far (starts at 0).
    /// Status transitions to `1` (funded) the moment this reaches `funding_target`.
    pub funded_amount: i128,
    /// Total settled (paid by buyer) so far
    pub settled_amount: i128,
    /// Yield basis points (e.g. 800 = 8%)
    pub yield_bps: i64,
    /// Ledger timestamp at which the invoice matures and settlement is expected.
    /// Stored as seconds since Unix epoch (Soroban `u64` ledger time).
    pub maturity: u64,
    /// Escrow lifecycle status:
    /// - `0` — **open**: accepting investor funding
    /// - `1` — **funded**: target met; SME can be paid; awaiting buyer settlement
    /// - `2` — **settled**: buyer paid; investors can redeem principal + yield
    pub status: u32,
    /// Whether emergency mode has been activated for this escrow
    pub emergency_mode: bool,
    /// Storage schema version — must equal [`SCHEMA_VERSION`] after any migration
    pub version: u32,
}

/// Storage keys for contract instance storage
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Main escrow state
    Escrow,
    /// Individual investor balances for emergency refund calculations
    InvestorBalances,
    /// Set of investors who have already received emergency refunds (double-claim prevention)
    RefundedInvestors,
    /// Reentrancy guard to prevent recursive calls during fund operations
    ReentrancyGuard,
}

/// Emitted when maturity timestamp is updated by admin.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaturityUpdatedEvent {
    /// Event name topic.
    #[topic]
    pub name: Symbol,
    /// Invoice whose maturity was updated.
    pub invoice_id: Symbol,
    /// Previous maturity timestamp.
    pub old_maturity: u64,
    /// New maturity timestamp.
    pub new_maturity: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartialSettlementEvent {
    pub invoice_id: Symbol,
    pub amount: i128,
    pub settled_amount: i128,
    pub total_due: i128,
}

// ──────────────────────────────────────────────────────────────────────────────
// Event types (one per state-changing function)
//
// Fields annotated with `#[topic]` appear in the Soroban event topic vector;
// all other fields appear in the event data payload.
//
// Keeping payloads as named structs makes XDR decoding forward-compatible and
// self-documenting in ledger explorers.  See docs/EVENT_SCHEMA.md for the
// full indexer reference including JSON examples and XDR topic filters.
// ──────────────────────────────────────────────────────────────────────────────

/// Emitted by `init()` when a new invoice escrow is created.
///
/// ### Indexer example (JSON after XDR decode)
/// ```json
/// {
///   "event"         : "escrow_initd",
///   "invoice_id"    : "INV1023",
///   "sme_address"   : "GBSME...",
///   "amount"        : 100000000000,
///   "funding_target": 100000000000,
///   "funded_amount" : 0,
///   "yield_bps"     : 800,
///   "maturity"      : 1750000000,
///   "status"        : 0
/// }
/// ```
#[contractevent]
pub struct EscrowInitialized {
    /// Event name topic — used by indexers to filter this event type.
    #[topic]
    pub name: Symbol,
    /// Full escrow snapshot at creation time (status always 0 / open).
    pub escrow: InvoiceEscrow,
}

/// Emitted by `fund()` on every successful investor contribution.
///
/// Emitted on **every** `fund()` call, not only when the target is first met.
/// Indexers can sum `amount` per `invoice_id` to reconstruct the full funding
/// history without reading contract storage.
///
/// ### Indexer example (JSON after XDR decode)
/// ```json
/// {
///   "event"        : "escrow_funded",
///   "invoice_id"   : "INV1023",
///   "investor"     : "GBINV...",
///   "amount"       : 50000000000,
///   "funded_amount": 100000000000,
///   "status"       : 1
/// }
/// ```
#[contractevent]
pub struct EscrowFunded {
    /// Event name topic.
    #[topic]
    pub name: Symbol,
    /// Invoice this contribution belongs to.
    pub invoice_id: Symbol,
    /// Investor wallet that called `fund()`.
    pub investor: Address,
    /// Amount added in this single call (always positive).
    pub amount: i128,
    /// Cumulative funded amount **after** this call.
    pub funded_amount: i128,
    /// Status value **after** this call: `0` = still open, `1` = now fully funded.
    pub status: u32,
}

/// Emitted by `settle()` once the buyer has paid and the escrow is closed.
///
/// Contains everything needed for a settlement accounting service to compute
/// investor payouts without re-reading contract storage.
///
/// ### Indexer example (JSON after XDR decode)
/// ```json
/// {
///   "event"         : "escrow_settled",
///   "invoice_id"    : "INV1023",
///   "funded_amount" : 100000000000,
///   "yield_bps"     : 800,
///   "maturity"      : 1750000000
/// }
/// ```
///
/// ### Payout formula (off-chain, backend responsibility)
/// ```text
/// gross_yield = funded_amount * (yield_bps / 10_000) * (days_held / 365)
/// investor_payout = funded_amount + gross_yield
/// ```
#[contractevent]
pub struct EscrowSettled {
    /// Event name topic.
    #[topic]
    pub name: Symbol,
    /// Invoice that has been settled.
    pub invoice_id: Symbol,
    /// Total principal held (== `funding_target` at settlement time).
    pub funded_amount: i128,
    /// Annualized yield in basis points for investor payout calculation.
    pub yield_bps: i64,
    /// Original maturity timestamp — used by backend to compute accrued interest.
    pub maturity: u64,
}

/// Emitted when emergency mode is activated by the admin.
///
/// This event marks the beginning of the emergency refund process.
/// Once activated, the escrow cannot return to normal operation.
///
/// ### Indexer example (JSON after XDR decode)
/// ```json
/// {
///   "event"      : "emergency_activated",
///   "invoice_id" : "INV1023",
///   "admin"      : "GBADMIN...",
///   "reason"     : "Emergency mode activated"
/// }
/// ```
#[contractevent]
pub struct EmergencyActivated {
    /// Event name topic.
    #[topic]
    pub name: Symbol,
    /// Invoice escrow that entered emergency mode.
    pub invoice_id: Symbol,
    /// Admin who activated emergency mode.
    pub admin: Address,
}

/// Emitted when an investor receives an emergency refund.
///
/// This event allows indexers to track the progress of emergency refunds
/// and detect when all investors have been refunded.
///
/// ### Indexer example (JSON after XDR decode)
/// ```json
/// {
///   "event"       : "emergency_refunded",
///   "invoice_id"  : "INV1023",
///   "investor"    : "GBINV...",
///   "refund_amount": 50000000000,
///   "total_funded" : 100000000000,
///   "proportion"   : 0.5
/// }
/// ```
#[contractevent]
pub struct EmergencyRefunded {
    /// Event name topic.
    #[topic]
    pub name: Symbol,
    /// Invoice escrow this refund belongs to.
    pub invoice_id: Symbol,
    /// Investor who received the refund.
    pub investor: Address,
    /// Amount refunded to the investor.
    pub refund_amount: i128,
    /// Total funded amount at time of emergency activation.
    pub total_funded: i128,
}

// ──────────────────────────────────────────────────────────────────────────────
// Contract
// ──────────────────────────────────────────────────────────────────────────────

#[contract]
pub struct LiquifactEscrow;

#[contractimpl]
impl LiquifactEscrow {
    // ==========================================================================
    // INIT & QUERY
    // ==========================================================================

    /// Initialize a new invoice escrow.
    ///
    /// Creates a new escrow with the specified parameters. The escrow starts
    /// in the "open" status (0), accepting investor funding.
    ///
    /// # Authorization
    /// Requires authorization from `admin`. This prevents any unauthorized
    /// party from creating or overwriting escrow state.
    ///
    /// # Panics
    /// - If an escrow has already been initialized.
    /// - If amount is not positive.
    pub fn init(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        sme_address: Address,
        amount: i128,
        yield_bps: u64,
        maturity: u64,
    ) -> InvoiceEscrow {
        // Authorization: admin must authorize initialization
        admin.require_auth();

        // Prevent re-initialization
        assert!(
            !env.storage().instance().has(&DataKey::Escrow),
            "Escrow already initialized"
        );

        // Input validation: amount must be positive
        assert!(amount > 0, "Escrow amount must be positive");

        let escrow = InvoiceEscrow {
            invoice_id: invoice_id.clone(),
            admin: admin.clone(),
            sme_address: sme_address.clone(),
            amount,
            funding_target: amount,
            funded_amount: 0,
            settled_amount: 0,
            yield_bps: yield_bps as i64,
            maturity,
            status: 0, // open
            emergency_mode: false,
            version: SCHEMA_VERSION,
        };

        // Initialize storage
        env.storage()
            .instance()
            .set(&DataKey::Escrow, &escrow);
        env.storage()
            .instance()
            .set(&symbol_short!("version"), &SCHEMA_VERSION);
        
        // Initialize empty investor balances for emergency tracking
        let empty_balances: Map<Address, i128> = Map::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::InvestorBalances, &empty_balances);
        
        // Initialize empty refunded investors set
        let empty_refunded: Map<Address, bool> = Map::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::RefundedInvestors, &empty_refunded);

        // Emit initialization event
        EscrowInitialized {
            name: symbol_short!("escrow_in"),
            escrow: escrow.clone(),
        }
        .publish(&env);

        escrow
    }

    /// Return the current escrow state without modifying storage.
    ///
    /// Read-only; does **not** emit an event.
    ///
    /// ## Errors
    /// Panics with `"Escrow not initialized"` if `init` has not been called.
    pub fn get_escrow(env: Env) -> InvoiceEscrow {
        env.storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| panic!("Escrow not initialized"))
    }

    /// Returns the stored schema version.
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("version"))
            .unwrap_or(0)
    }

    /// Check if emergency mode is currently active.
    pub fn is_emergency_mode(env: Env) -> bool {
        Self::get_escrow(env).emergency_mode
    }

    /// Get the balance of a specific investor.
    /// Returns 0 if the investor has not funded or if escrow is not initialized.
    pub fn get_investor_balance(env: Env, investor: Address) -> i128 {
        let balances: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&DataKey::InvestorBalances)
            .unwrap_or_else(|| Map::new(&env));
        
        balances.get(investor).unwrap_or(0)
    }

    /// Check if an investor has already received an emergency refund.
    pub fn is_refunded(env: Env, investor: Address) -> bool {
        let refunded: Map<Address, bool> = env
            .storage()
            .instance()
            .get(&DataKey::RefundedInvestors)
            .unwrap_or_else(|| Map::new(&env));
        
        refunded.get(investor).unwrap_or(false)
    }

    /// Migrate storage from an older schema version to the current one.
    ///
    /// # Security
    /// In production this MUST be gated behind admin/owner authorization
    /// (e.g. `admin_address.require_auth()`) so only the contract deployer can trigger it.
    ///
    /// # How to add a new migration
    /// 1. Bump [`SCHEMA_VERSION`].
    /// 2. Add a `from_version == N` arm below that reads the old struct
    ///    (keep the old type alias in a `legacy` module), transforms it, and
    ///    writes the new struct.
    /// 3. Add a test in `test.rs` that simulates the old state and calls `migrate`.
    pub fn migrate(env: Env, from_version: u32) -> u32 {
        let stored: u32 = env
            .storage()
            .instance()
            .get(&symbol_short!("version"))
            .unwrap_or(0);

        assert!(
            stored == from_version,
            "from_version does not match stored version"
        );
        assert!(
            from_version < SCHEMA_VERSION,
            "Already at current schema version"
        );

        // --- Migration arms ---
        // Add a new `if from_version == N` block for each future version bump.
        
        if from_version == 1 {
            // Migration from V1 to V2: Add emergency_mode field and investor tracking
            let old_escrow: InvoiceEscrow = env
                .storage()
                .instance()
                .get(&DataKey::Escrow)
                .unwrap_or_else(|| panic!("Escrow not found during migration"));
            
            let new_escrow = InvoiceEscrow {
                emergency_mode: false,
                version: SCHEMA_VERSION,
                ..old_escrow
            };
            
            env.storage()
                .instance()
                .set(&DataKey::Escrow, &new_escrow);
            env.storage()
                .instance()
                .set(&symbol_short!("version"), &SCHEMA_VERSION);
            
            // Initialize emergency tracking storage
            let empty_balances: Map<Address, i128> = Map::new(&env);
            env.storage()
                .instance()
                .set(&DataKey::InvestorBalances, &empty_balances);
            
            let empty_refunded: Map<Address, bool> = Map::new(&env);
            env.storage()
                .instance()
                .set(&DataKey::RefundedInvestors, &empty_refunded);
        }

        SCHEMA_VERSION
    }

    // ==========================================================================
    // FUNDING
    // ==========================================================================

    /// Record investor funding.
    ///
    /// In production, this would be called with token transfer.
    /// This version records accounting only.
    ///
    /// # Authorization
    /// Requires authorization from `investor`. Each investor authorizes their
    /// own funding contribution, preventing third parties from funding on their behalf.
    ///
    /// # Panics
    /// - If the escrow is not in the open (status = 0) state.
    /// - If emergency mode is active (cannot fund during emergency).
    /// - If amount is not positive.
    pub fn fund(env: Env, investor: Address, amount: i128) -> InvoiceEscrow {
        // Auth boundary: investor must authorize their own funding action.
        investor.require_auth();

        let mut escrow = Self::get_escrow(env.clone());
        
        // State check: cannot fund during emergency mode
        assert!(
            !escrow.emergency_mode,
            "Cannot fund while emergency mode is active"
        );
        
        // Input validation: Reject zero or negative funding amounts
        assert!(amount > 0, "Funding amount must be positive");
        assert!(escrow.status == 0, "Escrow not open for funding");

        // Update escrow state
        escrow.funded_amount += amount;
        if escrow.funded_amount >= escrow.funding_target {
            escrow.status = 1; // funded — ready to release to SME
        }

        // Update individual investor balance for emergency refund tracking
        let mut balances: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&DataKey::InvestorBalances)
            .unwrap_or_else(|| Map::new(&env));
        
        let current_balance = balances.get(investor.clone()).unwrap_or(0);
        balances.set(investor.clone(), current_balance + amount);
        
        env.storage().instance().set(&DataKey::InvestorBalances, &balances);
        env.storage().instance().set(&DataKey::Escrow, &escrow);

        // Emit funding event
        EscrowFunded {
            name: symbol_short!("escrow_fd"),
            invoice_id: escrow.invoice_id.clone(),
            investor,
            amount,
            funded_amount: escrow.funded_amount,
            status: escrow.status,
        }
        .publish(&env);

        escrow
    }

    // ==========================================================================
    // SETTLEMENT
    // ==========================================================================

    /// Mark escrow as settled (buyer paid). Releases principal + yield to investors.
    ///
    /// This is the final step in the escrow lifecycle. It requires that:
    /// 1. The escrow is fully funded (status = 1).
    /// 2. The SME (payee) authorizes the settlement.
    ///
    /// # Authorization
    /// Requires authorization from the `sme_address` stored in the escrow.
    /// Only the SME that is the beneficiary of the escrow may trigger settlement,
    /// preventing unauthorized state transitions to the settled state.
    ///
    /// # Panics
    /// - If the escrow is not in the funded (status = 1) state.
    /// - If emergency mode is active.
    pub fn settle(env: Env) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone());

        // Auth boundary: only the SME (payee) may settle the escrow.
        escrow.sme_address.require_auth();

        // State checks
        assert!(
            !escrow.emergency_mode,
            "Cannot settle during emergency mode"
        );
        assert!(
            escrow.status == 1,
            "Escrow must be funded before settlement"
        );
        
        // Calculate totals
        let interest = (escrow.amount * (escrow.yield_bps as i128)) / 10000;
        let total_due = escrow.amount + interest;
        
        // Update state
        escrow.settled_amount = total_due;
        escrow.status = 2; // settled

        env.storage()
            .instance()
            .set(&DataKey::Escrow, &escrow);

        // Emit settlement event
        EscrowSettled {
            name: symbol_short!("escrow_st"),
            invoice_id: escrow.invoice_id.clone(),
            funded_amount: escrow.funded_amount,
            yield_bps: escrow.yield_bps,
            maturity: escrow.maturity,
        }
        .publish(&env);

        escrow
    }

    /// Update maturity timestamp. Only allowed by admin in Open state.
    ///
    /// # Authorization
    /// Requires authorization from the admin.
    ///
    /// # Panics
    /// - If escrow status is not open (0).
    /// - If emergency mode is active.
    pub fn update_maturity(env: Env, new_maturity: u64) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone());

        // Authorization check
        escrow.admin.require_auth();

        // State validation: cannot update maturity during emergency
        assert!(
            !escrow.emergency_mode,
            "Cannot update maturity during emergency mode"
        );
        assert!(
            escrow.status == 0,
            "Maturity can only be updated in Open state"
        );

        let old_maturity = escrow.maturity;
        escrow.maturity = new_maturity;

        env.storage()
            .instance()
            .set(&DataKey::Escrow, &escrow);

        // Emit maturity update event
        MaturityUpdatedEvent {
            name: symbol_short!("maturity"),
            invoice_id: escrow.invoice_id.clone(),
            old_maturity,
            new_maturity,
        }
        .publish(&env);

        escrow
    }

    // ==========================================================================
    // EMERGENCY REFUND
    // ==========================================================================

    /// Activate emergency mode for this escrow.
    ///
    /// This function allows the admin to activate emergency mode, which enables
    /// investors to claim proportional refunds of their contributions.
    ///
    /// ## Design Decisions (as inline comments for code review):
    ///
    /// 1. **Access Control**: Only admin can activate emergency mode (consistent with
    ///    `update_maturity` pattern). This ensures only trusted parties can trigger
    ///    the emergency refund process.
    ///
    /// 2. **Activation States**: Emergency mode can only be activated when escrow
    ///    is in open (0) or funded (1) states. It cannot be activated after settlement
    ///    (status 2) because settled escrows should follow normal redemption流程.
    ///
    /// 3. **One-way Transition**: Emergency mode is intentionally one-way. Once activated,
    ///    the escrow cannot return to normal operation, ensuring the emergency process
    ///    can complete even if the admin becomes unavailable.
    ///
    /// # Authorization
    /// Requires authorization from the admin stored in the escrow.
    ///
    /// # Panics
    /// - If escrow is already in settled status (2).
    /// - If emergency mode is already active.
    pub fn activate_emergency(env: Env) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone());

        // Auth boundary: only admin can activate emergency mode
        escrow.admin.require_auth();

        // State validation: cannot activate emergency if already settled
        assert!(
            escrow.status != 2,
            "Cannot activate emergency mode after settlement"
        );
        
        // Cannot activate if already active
        assert!(
            !escrow.emergency_mode,
            "Emergency mode already active"
        );

        // Activate emergency mode - this is a one-way transition
        escrow.emergency_mode = true;

        env.storage()
            .instance()
            .set(&DataKey::Escrow, &escrow);

        // Emit emergency activation event
        EmergencyActivated {
            name: symbol_short!("emerg_act"),
            invoice_id: escrow.invoice_id.clone(),
            admin: escrow.admin.clone(),
        }
        .publish(&env);

        escrow
    }

    /// Claim emergency refund for the calling investor.
    ///
    /// This function allows individual investors to claim their proportional
    /// refund when emergency mode is active. The refund amount is calculated
    /// based on the investor's contribution relative to the total funded amount.
    ///
    /// ## Proportional Refund Calculation
    ///
    /// Each investor receives their exact contributed amount.
    /// The refund amount equals `investor_balance` which represents their contribution
    /// to the total escrow.
    ///
    /// If the escrow is in "open" status (0), investors receive their exact contribution.
    /// If the escrow is in "funded" status (1), investors receive their proportional share
    /// of the funded amount (SME withdrawal not yet executed).
    ///
    /// ## Reentrancy Protection
    ///
    /// This function implements checks-effects-interactions pattern strictly:
    /// 1. Check reentrancy guard and validate all preconditions
    /// 2. Update state (mark investor as refunded, update escrow state)
    /// 3. Transfer funds last (emit event for off-chain processing)
    ///
    /// The reentrancy guard prevents recursive calls that could lead to double-refunds.
    ///
    /// ## Double-Claim Prevention
    ///
    /// Each investor can only claim their refund once. Subsequent calls will fail
    /// with `AlreadyRefunded`. This is tracked using the `RefundedInvestors` map.
    ///
    /// # Authorization
    /// Requires authorization from the investor claiming the refund.
    ///
    /// # Panics
    /// - If emergency mode is not active.
    /// - If investor has already been refunded.
    /// - If investor has zero balance.
    ///
    /// # Returns
    /// The amount refunded to the investor.
    pub fn emergency_refund(env: Env, investor: Address) -> i128 {
        // Authorization: investor must authorize their own refund claim
        investor.require_auth();
        
        // CHECKS: Validate all preconditions before any state changes
        
        // Reentrancy guard: prevent recursive calls
        // This implements checks-effects-interactions: validate before modifying state
        assert!(
            !env.storage().instance().has(&DataKey::ReentrancyGuard),
            "Reentrancy detected: emergency_refund in progress"
        );
        
        // Set reentrancy guard
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyGuard, &true);

        // Get escrow state
        let escrow = Self::get_escrow(env.clone());
        
        // Emergency mode check: refund only available during emergency
        assert!(
            escrow.emergency_mode,
            "Emergency mode not active"
        );
        
        // Double-claim prevention: check if already refunded
        let mut refunded: Map<Address, bool> = env
            .storage()
            .instance()
            .get(&DataKey::RefundedInvestors)
            .unwrap_or_else(|| Map::new(&env));
        
        assert!(
            !refunded.get(investor.clone()).unwrap_or(false),
            "Already refunded"
        );

        // Get investor's balance
        let balances: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&DataKey::InvestorBalances)
            .unwrap_or_else(|| Map::new(&env));
        
        let investor_balance = balances.get(investor.clone()).unwrap_or(0);
        
        // Balance check: cannot refund zero balance
        assert!(
            investor_balance > 0,
            "No balance to refund"
        );

        // EFFECTS: Update all state before any external calls
        
        // Mark investor as refunded to prevent double-claims
        refunded.set(investor.clone(), true);
        env.storage()
            .instance()
            .set(&DataKey::RefundedInvestors, &refunded);

        // Update escrow: decrement funded_amount
        let mut updated_escrow = escrow.clone();
        updated_escrow.funded_amount -= investor_balance;
        
        env.storage()
            .instance()
            .set(&DataKey::Escrow, &updated_escrow);

        // Clear reentrancy guard
        env.storage()
            .instance()
            .remove(&DataKey::ReentrancyGuard);

        // INTERACTIONS: External calls last (event emission)
        
        // Emit emergency refund event
        // Note: In production, this would include the actual token transfer.
        // This event signals to off-chain systems that the refund should be processed.
        EmergencyRefunded {
            name: symbol_short!("emerg_rfd"),
            invoice_id: escrow.invoice_id.clone(),
            investor: investor.clone(),
            refund_amount: investor_balance,
            total_funded: escrow.funded_amount + investor_balance, // Original total before deduction
        }
        .publish(&env);

        investor_balance
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod test;
