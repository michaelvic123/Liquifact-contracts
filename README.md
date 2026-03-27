# LiquiFact Escrow Contract

Soroban smart contracts for **LiquiFact** on Stellar. This repository currently
contains the `escrow` contract crate and its supporting documentation.

## Storage Schema And Upgrade Compatibility

**Risk:**
- Unauthorized caller could attempt to `fund`, `confirm_payment`, `settle`, or `redeem`

- the raw storage inventory actually declared or referenced by source
- the narrative schema view reviewers should rely on when evaluating upgrades
- known divergences where code and documentation cannot yet be made perfectly clean

**Mitigation (Current):**
- Role-based authorization:
  - `fund`: the caller must be the `investor`
  - `confirm_payment`: the configured `buyer_address` must authorize
  - `settle`: the configured `sme_address` must authorize
  - `redeem`: the caller must be the `investor`

**Recommended Controls:**
- Keep read-only queries (`get_investor_position`) free of auth requirements, while ensuring state-changing calls are auth-gated.

---

### 2. Arithmetic Risks (Overflow / Underflow)

**Risk:**
- `funded_amount += amount` may overflow `i128`

---

- instance storage references currently use the literal keys `"escrow"` and `"version"`
- the persisted escrow record is the `InvoiceEscrow` struct as declared in source
- any API parameter or helper that is not written into storage is excluded from the
  persisted schema narrative and called out separately as divergence

### Raw storage inventory

#### Instance storage keys referenced by source

| Key | Source usage | Stored value |
|---|---|---|
| `"escrow"` | `get_escrow`, `init`, `fund`, `settle`, `update_maturity`, `withdraw`, migration example | `InvoiceEscrow` |
| `"version"` | `init`, `get_version`, `migrate` | `u32` schema version |

#### Persisted struct fields exactly as declared in `InvoiceEscrow`

| Command                    | Description                   |
|----------------------------|-------------------------------|
| `cargo build`              | Build all contracts           |
| `cargo test`               | Run unit tests                |
| `cargo fmt`                | Format code                   |
| `cargo fmt -- --check`     | Check formatting (used in CI) |

### Narrative schema view

The contract is trying to persist a single escrow snapshot plus an explicit
schema version:

- `"escrow"` stores the full invoice escrow snapshot
- `"version"` stores the current schema version independently for upgrade checks

For reviewer guidance, the intended persisted escrow fields are:

| Field | Intended meaning |
|---|---|
| `invoice_id` | Short invoice identifier used as part of the escrow record |
| `admin` | Administrative address intended to control privileged maintenance actions |
| `sme_address` | SME beneficiary address |
| `amount` | Principal amount |
| `funding_target` | Funding threshold, currently initialized from `amount` |
| `funded_amount` | Running funded total |
| `settled_amount` | Running settled total; source support exists but behavior is still inconsistent |
| `yield_bps` | Yield in basis points |
| `maturity` | Maturity timestamp |
| `status` | Lifecycle state flag |
| `version` | Persisted schema version, expected to track `SCHEMA_VERSION` |

### Status values

- **init** — Create an invoice escrow (invoice id, SME address, amount, yield bps, maturity). Emits `init` event.
- **get_escrow** — Read current escrow state.
- **get_version** — Return the stored schema version number.
- **confirm_payment** — Buyer confirms repayment (sets `is_paid = true`).
- **fund** — Record investor funding; status becomes "funded" when target is met.
- **settle** — Mark escrow as settled (buyer paid; investors receive principal + yield).
- **redeem** — Mark an investor’s claim as redeemed (accounting only).
- **get_investor_position** — Read-only investor position query (issue #45).
- **migrate** — Upgrade storage from an older schema version to the current one (see below).

### Maturity gate

`settle` enforces two guards before advancing status to `settled (2)`:

1. **Funding check** — `status` must equal `1` (fully funded). Attempting to settle an unfunded escrow panics with `"Escrow must be funded before settlement"`.
2. **Time check** — `env.ledger().timestamp()` must be **≥ `maturity`**. Attempting to settle before the invoice is due panics with `"Cannot settle before maturity timestamp"`.

`env.ledger().timestamp()` is the canonical Soroban on-chain clock. It is set by the Stellar network and **cannot be manipulated by the contract caller**, making it safe to use as a time gate.

| Ledger time vs maturity | Status | Result |
|-------------------------|--------|--------|
| `now < maturity`        | funded | panic — premature settlement blocked |
| `now == maturity`       | funded | success |
| `now > maturity`        | funded | success |
| any                     | open   | panic — not yet funded |

Setting `maturity = 0` effectively disables the time lock (any timestamp ≥ 0).

---

### Events

The contract emits Soroban events on every state-changing call, enabling off-chain indexers and analytics.

| Method   | Topics                   | Payload fields                                   |
|----------|--------------------------|--------------------------------------------------|
| `init`   | `["init", invoice_id]`   | `sme_address`, `amount`, `yield_bps`, `maturity` |
| `fund`   | `["fund", invoice_id]`   | `investor`, `amount`, `funded_amount`, `status`  |
| `settle` | `["settle", invoice_id]` | `sme_address`, `amount`, `yield_bps`             |

All payload types (`InitEvent`, `FundEvent`, `SettleEvent`) are exported `#[contracttype]` structs — see [`escrow/src/lib.rs`](escrow/src/lib.rs) for full field documentation.

The `invoice_id` in the topic allows indexers to filter events by invoice without decoding the payload.

- Re-initialization protection is intended, but the current guard references undefined helper machinery.
- Upgrade paths should be admin-gated before production deployment.
- Migration code must never silently drop or reinterpret historical fields.
- Reviewers should treat source drift as a signal to preserve backward decoders and add focused migration tests before any schema evolution lands.

State-changing methods enforce Stellar auth using `require_auth()`:
- `fund` requires authorization from the caller (the `investor`).
- `confirm_payment` requires authorization from the configured `buyer_address`.
- `settle` requires authorization from the configured `sme_address`.
- `redeem` requires authorization from the caller (the `investor`).

Read-only methods (including `get_investor_position`) do not require auth, and return only public accounting data (amounts, escrow status, and claim flags).

- Legacy singleton value exists under key `"escrow"`
- Legacy `invoice_id` must match the provided `invoice_id`
- Target keyed invoice must not already exist

## Contract lifecycle

## Funding Constraints
- **Minimum Funding:** All funding amounts must be strictly greater than zero ($> 0$). 
- **Initialization:** Escrow creation will fail if the target amount is not positive.
- **Integer Safety:** Uses `checked_add` to prevent overflow during funded amount accounting.
- **Governance Controls (Target Update):** The funding target size (`amount`) can be modified by the initialized `admin`. It enforces strict governance constraints: it can only be modified when the escrow is `Open` (status = 0), the new target must be strictly positive, and it can never be less than the existing `funded_amount`.

- `0` open
- `1` funded
- `2` settled

Main methods:

- `init`
- `get_escrow`
- `list_invoices`
- `fund`
- `settle`
- `update_maturity`
- `transfer_admin`
- `migrate_singleton`

---

---

## Invariants

- `funded_amount <= funding_target` (soft enforced)
- `status transitions`: 0 → 1 → 2
- Cannot settle before funded
| Step | Command | Fails if… |
|------|---------|-----------|
| Format | `cargo fmt --all -- --check` | any file is not formatted |
| Build | `cargo build` | compilation error |
| Tests | `cargo test` | any test fails |
| Coverage | `cargo llvm-cov --features testutils --fail-under-lines 95` | line coverage < 95% |

### Coverage gate

The pipeline uses [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov)
to measure line coverage and fail when it drops below **95%**.

```bash
cargo install cargo-llvm-cov
rustup component add llvm-tools-preview
cargo llvm-cov --features testutils --fail-under-lines 95 --summary-only
```

## Test & Coverage

- `cargo test`: all unit tests passed (`12` passed).
- `cargo llvm-cov`: `TOTAL` line coverage `99.40%` (831 lines, 5 missed), meeting the CI threshold of `≥ 95%`.

Security note: `get_investor_position` is read-only (no Stellar auth required) and returns only on-chain accounting data (addresses, amounts, claim flags). It does not expose any off-chain personal information.

Keep formatting, tests, and coverage passing before opening a PR.

---

## Contributing

1. **Fork** the repo and clone your fork.
2. **Create a branch** from `main`: `git checkout -b feature/your-feature` or `fix/your-fix`.
3. **Setup**: ensure Rust stable is installed; run `cargo build` and `cargo test`.
4. **Make changes**:
   - Follow existing patterns in `escrow/src/lib.rs`.
   - Add or update tests in `escrow/src/test.rs`.
   - Format with `cargo fmt`.
5. **Verify locally**:
   - `cargo fmt --all -- --check`
   - `cargo build`
   - `cargo test --features testutils`
6. **Commit** with clear messages (e.g. `feat(escrow): X`, `test(escrow): Y`).
7. **Push** to your fork and open a **Pull Request** to `main`.
8. Wait for CI and address review feedback.

We welcome new contracts (e.g. settlement, tokenization helpers), tests, and docs that align with LiquiFact's invoice financing flow.

---

## Future Improvements

- Multi-escrow support
- Role-based access control
- Token integration
- Event emission
- Formal verification

## Emergency Refund Mechanism

Emergency mode provides a safe pathway to return funds to investors when normal settlement cannot proceed (e.g., legal dispute, operational failure, or suspected fraud). It follows the same access control, naming, and state-machine patterns as the rest of the contract.

### When It Can Be Activated
- Escrow status is open (0) or funded (1).
- Not available after settlement (2).
- One-way transition: once activated, the escrow remains in emergency mode.

### Who Can Activate
- Admin only. The stored admin address must authorize the call. This mirrors the access control pattern used in update_maturity.

### How Refunds Are Calculated
- Each investor receives a refund equal to their recorded contribution balance.
- Balances are tracked per investor during fund() in instance storage.
- This is equivalent to a proportional distribution because the total of all investor balances equals the funded amount at the time of activation.

### How Investors Claim
1. Wait for the admin to activate emergency mode.
2. Call emergency_refund(investor) with your address as the caller.
3. The contract verifies:
   - Emergency mode is active.
   - You are authorized as the investor (require_auth).
   - You have not already claimed.
   - Your recorded balance is greater than zero.
4. Your individual refund amount is returned and an EmergencyRefunded event is emitted for audit/indexing.
5. You can verify the claim state with is_refunded(address) or your tracked balance with get_investor_balance(address).

### Security Considerations
- Checks–Effects–Interactions:
  - Checks: validate emergency mode, investor auth, not-refunded, and non-zero balance.
  - Effects: mark the investor as refunded and update escrow accounting before any external interaction.
  - Interactions: emit EmergencyRefunded event last. In production integrations, token transfers should also occur last.
- Double-claim prevention:
  - A RefundedInvestors map marks claimants so repeated calls are rejected.
- Reentrancy protection:
  - A simple storage-based guard prevents re-entrant execution of the refund flow and is cleared after each successful refund.
- Authorization:
  - activate_emergency requires admin authorization; emergency_refund requires the investor’s authorization.
