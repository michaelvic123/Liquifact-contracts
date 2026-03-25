# LiquiFact Contracts

Soroban smart contracts for **LiquiFact** — the global invoice liquidity network on Stellar. This repo contains the **escrow** contract that holds investor funds for tokenized invoices until settlement.

Part of the LiquiFact stack: **frontend** (Next.js) | **backend** (Express) | **contracts** (this repo).

---

## Prerequisites

- **Rust** 1.70+ (stable)
- **Soroban CLI** (optional, for deployment): [Stellar Soroban docs](https://developers.stellar.org/docs/smart-contracts/getting-started/soroban-cli)

For CI and local checks you only need Rust and `cargo`.

---

## Setup

1. **Clone the repo**

   ```bash
   git clone <this-repo-url>
   cd liquifact-contracts
   ```

2. **Build**

   ```bash
   cargo build
   ```

3. **Run tests**

   ```bash
   cargo test
   ```

---

## Development

| Command           | Description                    |
|-------------------|--------------------------------|
| `cargo build`     | Build all contracts            |
| `cargo test`      | Run unit tests                 |
| `cargo fmt`       | Format code                    |
| `cargo fmt -- --check` | Check formatting (used in CI) |

---

## Project structure

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

### Escrow contract (high level)

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

## Admin transfer (governance rotation)

Both `LiquifactEscrow` and `EscrowFactory` support secure rotation of the governance address via `transfer_admin`.

### Entry points

| Contract          | Signature                                              | Auth required    |
|-------------------|--------------------------------------------------------|------------------|
| `LiquifactEscrow` | `transfer_admin(new_admin)`                            | current `admin`  |
| `EscrowFactory`   | `transfer_admin(invoice_id, new_admin)`                | current `admin`  |

### Guards against accidental lockout

| Check                              | Error message                              |
|------------------------------------|--------------------------------------------|
| Caller must be current admin       | Auth failure (Soroban native)              |
| `new_admin` ≠ current admin        | `"New admin must differ from current admin"` |
| Escrow must be initialized / exist | `"Escrow not initialized"` / `"Escrow not found for invoice"` |

### Event

On every successful transfer both contracts emit:

```
topics: ("admin", "transfer")
data:   (old_admin: Address, new_admin: Address)
```

Off-chain observers can subscribe to this event to track governance rotation without polling storage.

### Security assumptions

- The caller supplies `new_admin` — ensure the target address is controlled before transferring.
- No time-lock or two-step confirmation is enforced on-chain; callers should implement off-chain confirmation if required.
- Once transferred, the old admin loses all governance privileges immediately.

---

## Escrow Factory pattern

The `EscrowFactory` contract (also in `escrow/src/lib.rs`) implements a **per-invoice factory** that registers and manages one isolated escrow record per invoice, removing the single-escrow-per-deployment limitation of the base `LiquifactEscrow` contract.

### Why a factory?

| Concern                  | `LiquifactEscrow` (single)        | `EscrowFactory` (factory)                  |
|--------------------------|-----------------------------------|--------------------------------------------|
| Escrows per contract     | 1                                 | Unlimited (one per invoice ID)             |
| State isolation          | Contract-level                    | Per-invoice, keyed by `Symbol`             |
| Enumeration              | Not supported                     | `list_invoices()` returns all IDs          |
| Operational overhead     | Deploy one contract per invoice   | Deploy once, register many invoices        |

### Factory entry points

| Function         | Auth required  | Description                                               |
|------------------|----------------|-----------------------------------------------------------|
| `create_escrow`  | `admin`        | Register a new per-invoice escrow; panics if ID exists    |
| `get_escrow`     | —              | Look up escrow state by invoice ID                        |
| `fund`           | `investor`     | Record investor contribution; flips status when target met|
| `settle`         | `sme_address`  | Mark a funded escrow as settled                           |
| `list_invoices`  | —              | Return all registered invoice IDs in creation order       |

### Storage layout

```
FactoryKey::Escrow(invoice_id)  →  InvoiceEscrow   (persistent, per-invoice)
FactoryKey::Registry            →  Vec<Symbol>     (persistent, ordered invoice list)
```

### Security assumptions

- `admin` is trusted to register legitimate invoices. Use a multisig address in production.
- Duplicate registration for the same `invoice_id` is blocked at the contract level.
- `settle` can only move status `1 → 2`; calling it on any other state panics.
- Funding one invoice never touches another invoice's storage slot.

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

## CI/CD

GitHub Actions runs on every push and pull request to `main`:

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
   - `cargo test`
6. **Commit** with clear messages (e.g. `feat(escrow): X`, `test(escrow): Y`).
7. **Push** to your fork and open a **Pull Request** to `main`.
8. Wait for CI and address review feedback.

We welcome new contracts (e.g. settlement, tokenization helpers), tests, and docs that align with LiquiFact’s invoice financing flow.

---

## License

MIT (see root LiquiFact project for full license).
