# LiquiFact Escrow Contract – Threat Model & Security Notes

## Overview
This contract manages invoice-backed escrow for SME financing:
- Investors fund invoices
- SME receives liquidity once funded
- Investors are repaid at settlement

---

## Threat Model

### 1. Unauthorized Access

**Risk:**
- Anyone can call `fund` or `settle`

**Impact:**
- Malicious settlement
- Fake funding events

**Mitigation (Current):**
- None (mock auth used in tests)

**Recommended Controls:**
- Require auth:
  - `fund`: investor must authorize
  - `settle`: only trusted role (e.g. admin/oracle)

---

### 2. Arithmetic Risks (Overflow / Underflow)

**Risk:**
- `funded_amount += amount` may overflow `i128`

**Impact:**
- Corrupted balances
- Incorrect settlement state

**Mitigation (Added):**
- Checked addition

---

### 3. Replay / Double Execution

**Risk:**
- `settle()` can be called repeatedly if state checks fail
- `init()` overwrites existing escrow

**Impact:**
- State corruption
- Funds mis-accounting

**Mitigation (Added):**
- Status guards
- Initialization guard

---

### 4. Storage Corruption / Assumptions

**Risk:**
- Single storage key (`escrow`)
- New init overwrites old escrow

**Impact:**
- Loss of previous escrow data

**Mitigation:**
- Assumes **1 escrow per contract instance**

**Recommended:**
- Use `invoice_id` as storage key

---

### 5. Invalid Input / Economic Attacks

**Risks:**
- Negative funding
- Zero funding
- Invalid maturity

**Mitigation (Added):**
- Input validation assertions

---

### 6. Time-based Attacks

**Risk:**
- Settlement before maturity
```
liquifact-contracts/
├── Cargo.toml           # Workspace definition
├── escrow/
│   ├── Cargo.toml       # Escrow contract crate
│   └── src/
│       ├── lib.rs       # LiquiFact escrow contract (init, fund, settle)
│       └── test.rs      # Unit tests
├── docs/
│   ├── openapi.yaml     # OpenAPI 3.1 specification
│   ├── package.json     # Test runner deps (AJV, js-yaml)
│   └── tests/
│       └── openapi.test.js  # Schema conformance tests (51 cases)
└── .github/workflows/
    └── ci.yml           # CI: fmt, build, test
```

**Mitigation (Recommended):**
- Enforce:

env.ledger().timestamp() >= maturity

- **init** — Create an invoice escrow (admin, invoice id, SME address, amount, yield bps, maturity). Requires `admin` authorization.
- **get_escrow** — Read current escrow state (no auth required).
- **fund** — Record investor funding; status becomes “funded” when target is met. Requires `investor` authorization.
- **settle** — Mark escrow as settled (buyer paid; investors receive principal + yield). Requires `sme_address` authorization.

### Authorization model

All sensitive state transitions are protected by Soroban's native [`require_auth`](https://developers.stellar.org/docs/smart-contracts/example-contracts/auth) mechanism.

| Function | Required Signer  | Rationale                                                  |
|----------|------------------|------------------------------------------------------------|
| `init`   | `admin`          | Prevents unauthorized escrow creation or re-initialization |
| `fund`   | `investor`       | Each investor authorizes their own contribution            |
| `settle` | `sme_address`    | Only the SME beneficiary may trigger settlement            |

`require_auth` integrates with Soroban's authorization framework: on-chain, the transaction must carry a valid signature (or sub-invocation auth) from the required address. In tests, `env.mock_all_auths()` satisfies all checks so happy-path logic can be verified independently of key management.

#### Security assumptions

- The `admin` address is trusted to create legitimate escrows. Rotate or use a multisig address in production.
- Re-initialization is blocked at the contract level (`"Escrow already initialized"` panic) regardless of who calls `init`.
- `settle` can only move status from `1 → 2`; calling it on an open or already-settled escrow panics.

---

## API documentation (OpenAPI)

The REST API surface is documented in [`docs/openapi.yaml`](docs/openapi.yaml) (OpenAPI 3.1).

### Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/v1/health` | — | Liveness probe |
| `GET` | `/v1/info` | — | API name, version, network |
| `GET` | `/v1/invoices` | JWT | List invoice summaries (paginated) |
| `GET` | `/v1/invoices/{invoiceId}` | JWT | Full escrow detail for one invoice |
| `POST` | `/v1/escrow` | JWT | Initialise a new invoice escrow |
| `POST` | `/v1/escrow/{invoiceId}/fund` | JWT | Record investor funding |
| `POST` | `/v1/escrow/{invoiceId}/settle` | JWT | Settle a funded escrow |

### Security

- All mutating and data endpoints require a `Bearer` JWT in the `Authorization` header.
- `/health` and `/info` are public (no auth required).
- Stellar addresses are validated as 56-char base32 (`[A-Z2-7]`) strings.
- Monetary amounts are always in stroops (smallest unit); `amount ≥ 1` is enforced.
- `yield_bps` is capped at `10000` (100 %) to prevent overflow.

### Running the schema conformance tests

```bash
cd docs
npm install
npm test
# tests 51 | pass 51 | fail 0
```

---

## Security Assumptions

- Soroban runtime guarantees:
- Deterministic execution
- Storage integrity
- Token transfers handled externally
- Off-chain systems validate invoice authenticity

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
| Coverage | `cargo llvm-cov --features testutils --fail-under-lines 95` | line coverage < 95 % |

### Coverage gate

The pipeline uses [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) (installed via `taiki-e/install-action`) to measure line coverage and hard-fail the job when it drops below **95 %**.

To run the coverage check locally:

```bash
# Install once
cargo install cargo-llvm-cov

# Run (requires llvm-tools-preview component)
rustup component add llvm-tools-preview
cargo llvm-cov --features testutils --fail-under-lines 95 --summary-only
```

Keep formatting, tests, and coverage passing before opening a PR.

---

## Test Coverage Notes

Edge cases covered:
- Funding beyond target
- Double settlement prevention
- Invalid initialization
- Arithmetic safety

---

## Funding Expiry

Each escrow includes a `funding_deadline`.

### Behavior

- If funding is not completed before deadline:
  → Escrow transitions to `EXPIRED (3)`

### Guarantees

- No funding allowed after expiry
- No settlement allowed after expiry
- Prevents capital lock

### Security Notes

- Expiry is enforced lazily (on interaction)
- No background execution required
- Timestamp sourced from ledger (trusted)

## Future Improvements

- Multi-escrow support
- Role-based access control
- Token integration
- Event emission
- Formal verification

