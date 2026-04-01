# LiquiFact Escrow Contract

Soroban smart contracts for LiquiFact, the invoice liquidity network on Stellar. This repository currently contains the `escrow` contract that holds investor funds for tokenized invoices until settlement.

### Per-instance funding asset and registry (issues #113, #116)

- Rust 1.70+ (stable)
- Soroban CLI (optional for deployment)

For local development and CI, Rust is enough.

### Treasury dust sweep (issue #107)

```bash
cargo build
cargo test
```

## Storage-only upgrade policy (additive fields)

**Compatible without redeploy** when you only:

- Add **new** `DataKey` variants and/or new `#[contracttype]` structs stored under **new** keys.
- Read new keys with `.get(...).unwrap_or(default)` so missing keys behave as “unset” on old deployments.

**Requires new deployment or explicit migration** when you:

- Change layout or meaning of an existing stored type (e.g. new required field on `InvoiceEscrow` without a migration that rewrites `DataKey::Escrow`).
- Rename or change the XDR shape of an existing `DataKey` variant used in production.

**Compatibility test plan (short):**

1. Deploy version _N_; exercise `init`, `fund`, `settle`.
2. Deploy version _N+1_ with only new optional keys; repeat flows; assert old instances still readable.
3. If `InvoiceEscrow` changes, add a migration test or document mandatory redeploy.

`migrate` today validates `from_version` against stored `DataKey::Version` and errors if no path is implemented.

### `DataKey` naming convention

| Command | Description |
|---|---|
| `cargo build` | Build the workspace |
| `cargo test` | Run unit tests |
| `cargo fmt` | Format code |
| `cargo fmt -- --check` | Check formatting |

## Release runbook: build, deploy, verify

**Who may deploy production:** only addresses and keys owned by LiquiFact governance (multisig / custody). Treat contract admin and deployer secrets as **highly sensitive**.

### Environment variables (example)

| Variable | Purpose |
|----------|---------|
| `STELLAR_NETWORK` | e.g. `TESTNET` / `PUBLIC` / custom Horizon passphrase |
| `SOROBAN_RPC_URL` | Soroban RPC endpoint |
| `SOURCE_SECRET` | Funding / deployer Stellar secret key (S ...) |
| `LIQUIFACT_ADMIN_ADDRESS` | Initial admin intended to control holds and funding target |

Exact CLI flags change between Soroban releases; always cross-check [Stellar Soroban docs](https://developers.stellar.org/docs/tools/soroban-cli/stellar-cli) for your installed `stellar` / `soroban` CLI version.

### Build WASM

```bash
rustup target add wasm32v1-none
cargo build --target wasm32v1-none --release
# Lint the escrow crate (mirrors CI)
cargo clippy -p escrow -- -D warnings

# Lint the entire workspace
cargo clippy --all-targets -- -D warnings
# Artifact (typical):
# target/wasm32v1-none/release/liquifact_escrow.wasm
```

## Escrow contract

- `init`: Create an invoice escrow.
- `get_escrow`: Read the current escrow state.
- `fund`: Record funding, track each investor's principal contribution, and mark the escrow funded once the target is reached.
- `settle`: Mark a funded escrow as settled.
- `get_investor_count`: Return the number of distinct investors recorded for the escrow.
- `get_investor_contribution`: Return the principal amount recorded for one investor.
- `max_investors`: Return the supported investor cap for one escrow.

## Storage guardrails

The escrow stores a per-investor contribution map inside the contract instance. That map is intentionally bounded.

- Supported investor cardinality: `128` distinct investors per escrow
- Product assumption: invoices that need more than `128` backers should be split across multiple escrows or a higher-level allocation flow
- Security goal: prevent denial-of-storage attacks that keep inserting new investor keys until a single contract-data entry becomes too large or too expensive to update

The regression tests in `escrow/src/test.rs` enforce these assumptions:

- The `129th` distinct investor is rejected.
- Re-funding an existing investor at the cap is still allowed.
- At `128` investors, the serialized investor map and escrow entry must stay below documented byte thresholds.
- The final insertion at the cap must stay within a bounded write footprint.

These limits are designed to keep the contract well below Soroban's contract-data entry limits and to catch future schema changes that would bloat per-investor storage.

## Security notes

- Funding amounts must be positive.
- Distinct investor growth is capped per escrow.
- Funding totals and investor balances use checked addition to avoid overflow.
- Storage-growth tests act as regression guards against accidental state bloat.

## CI

Run these before opening a PR:

```bash
cargo fmt --all -- --check
cargo build
cargo test
```

## Test organization

Escrow tests are organized by feature area under [`escrow/src/test/`](escrow/src/test):

- `init.rs` covers initialization, invoice-id validation, getters, and init-shaped baselines
- `funding.rs` covers funding, contribution accounting, snapshots, and tier selection
- `settlement.rs` covers settlement, withdrawal, investor claims, maturity boundaries, and dust sweep
- `admin.rs` covers admin-governed state changes, legal hold, migration guards, and collateral metadata
- `integration.rs` covers external token-wrapper assumptions and metadata-only integration checks
- `properties.rs` contains proptest-based invariants

Shared helpers remain in [`escrow/src/test.rs`](escrow/src/test.rs). Each test creates its own fresh
`Env` and local setup so feature modules do not rely on hidden cross-test state.

---

## Architecture Decision Records

Core design decisions are captured in [`docs/adr/`](docs/adr/):

| ADR | Decision |
|-----|----------|
| [ADR-001](docs/adr/ADR-001-state-model.md) | Escrow state model (`status` 0–3, forward-only transitions) |
| [ADR-002](docs/adr/ADR-002-auth-boundaries.md) | Authorization boundaries per role (admin, SME, investor, treasury) |
| [ADR-003](docs/adr/ADR-003-settlement-flow.md) | Two-phase settlement flow and funding-close snapshot |
| [ADR-004](docs/adr/ADR-004-legal-hold.md) | Legal / compliance hold mechanism |
| [ADR-005](docs/adr/ADR-005-tiered-yield.md) | Optional tiered yield and per-investor commitment locks |
| [ADR-006](docs/adr/ADR-006-dust-sweep-and-token-safety.md) | Treasury dust sweep and SEP-41 token safety wrapper |

## Token integration security checklist

See [`docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`](docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md) for the supported token assumptions, explicit unsupported token warnings, and the integration-layer responsibilities required when this escrow contract interacts with external token contracts.

## Security notes

- **Auth:** state-changing entrypoints use `require_auth()` for the appropriate role (admin, SME, investor, **treasury** for dust sweep).
- **Legal hold:** is governance-controlled; misuse risk is mitigated by using a multisig `admin` and operational policy.
- **Collateral record:** is not proof of encumbrance until a future version explicitly enforces token transfers.
- **Token integration:** external token transfers and token safety validation must live in the integration layer; this contract stores only numeric amount state and collateral metadata.
- **Overflow:** `fund` uses `checked_add` on `funded_amount`.
- **Dust sweep:** gated on **terminal** escrow status, per-call **cap** ([`MAX_DUST_SWEEP_AMOUNT`]), actual **balance**, **legal hold**, and **treasury** auth; only the **configured** SEP-41 token is transferred, with **post-transfer balance equality** checks in [`external_calls`](escrow/src/external_calls.rs). Wrong-asset or oversized balances still require operational discipline — the hook is not a general-purpose withdrawal for live liabilities.
- **Tiered yield / claim locks:** first-deposit discipline (`fund` vs `fund_with_commitment`) prevents changing an investor’s tier after their initial leg; claim timestamps are ledger-based.
- **Funding snapshot:** single-write immutability avoids shifting pro-rata denominators after close.
- **Registry ref:** stored for discoverability only; it must not be used as an authority without verifying behavior of the registry contract off-chain or in a dedicated integration.

### Contract type clone/derive safety

- `DataKey` keeps `Clone` because key wrappers are reused for storage get/set paths.
- `InvoiceEscrow` and `SmeCollateralCommitment` intentionally do **not** derive `Clone`; this prevents accidental full-state duplication in hot paths.
- `InvoiceEscrow` and `SmeCollateralCommitment` derive `PartialEq` for deterministic state assertions in tests and `Debug` for failure diagnostics.
- `init` publishes `EscrowInitialized` from stored state instead of cloning the in-memory escrow snapshot, reducing avoidable copy overhead.

---

## Contributing

MIT
