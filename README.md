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

| Command                    | Description                       |
|----------------------------|-----------------------------------|
| `cargo build`              | Build all contracts               |
| `cargo test`               | Run unit tests                    |
| `cargo fmt`                | Format code                       |
| `cargo fmt -- --check`     | Check formatting (used in CI)     |

---

## Project structure

```
liquifact-contracts/
├── Cargo.toml           # Workspace definition
├── escrow/
│   ├── Cargo.toml       # Escrow contract crate
│   └── src/
│       ├── lib.rs       # LiquiFact escrow contract (init, fund, settle, version)
│       └── test.rs      # Unit tests (≥ 95 % coverage)
└── .github/workflows/
    └── ci.yml           # CI: fmt, build, test
```

### Escrow contract (high level)

| Method              | Auth    | Description                                                                   |
|---------------------|---------|-------------------------------------------------------------------------------|
| `version`           | Anyone  | **Read-only.** Returns semantic version string (`"MAJOR.MINOR.PATCH"`).      |
| `init_with_admin`   | Anyone  | Initialise governance state; sets the admin address and starts unpaused.     |
| `pause`             | Admin   | Emergency-stop: blocks `fund` and `settle` until unpaused.                   |
| `unpause`           | Admin   | Lift the emergency stop; re-enables `fund` and `settle`.                     |
| `is_paused`         | Anyone  | **Read-only.** Returns `true` while the contract is in emergency-stop state. |
| `init`              | Anyone  | Create an invoice escrow (invoice id, SME address, amount, yield bps, maturity). |
| `get_escrow`        | Anyone  | **Read-only.** Returns current escrow state.                                 |
| `fund`              | Anyone  | Record investor funding; blocked when paused; status → `Funded` when target met. |
| `settle`            | Anyone  | Mark escrow settled; blocked when paused; requires `Funded` status.          |

---

## Emergency Pause Mechanism (`pause` / `unpause` / `is_paused`)

### Overview

The contract exposes a governance-controlled pause switch (Issue #24) for
incident response. A designated **admin** address can halt all investor
operations instantly and restore them once the incident is resolved.

```rust
let admin = Address::from_string("GADMIN...");
let mut state = EscrowContract::init_with_admin(admin.clone());

// Incident detected — block fund and settle immediately.
EscrowContract::pause(&mut state, &admin);
assert!(EscrowContract::is_paused(&state));

// Incident resolved — restore normal operation.
EscrowContract::unpause(&mut state, &admin);
assert!(!EscrowContract::is_paused(&state));
```

### Pause semantics

| State    | `fund()` | `settle()` | `pause()`  | `unpause()` | `is_paused()` |
|----------|----------|------------|------------|-------------|---------------|
| Unpaused | ✅        | ✅          | ✅ (admin) | ❌ panics   | `false`       |
| Paused   | ❌ panics | ❌ panics   | ❌ panics  | ✅ (admin)  | `true`        |

### Security properties

| Property                | Detail                                                                    |
|-------------------------|---------------------------------------------------------------------------|
| Admin-only access       | Any non-admin caller is rejected before any state is changed.             |
| Idempotency guards      | Double-pause and double-unpause both panic — surfaces operator mistakes.  |
| Read-only methods unblocked | `version`, `get_escrow`, `is_paused` are always accessible.          |
| No privilege escalation | Admin is set once at `init_with_admin`; cannot be rotated in this version.|
| No silent no-ops        | Every pause/unpause call either mutates state or panics — never silently skips. |

### Break-glass workflow

1. Admin detects incident → calls `pause(state, admin)`.
2. All `fund` and `settle` calls revert with `"contract is paused"`.
3. Incident resolved → admin calls `unpause(state, admin)`.
4. Normal operations resume immediately.
5. Tooling can poll `is_paused()` (read-only, zero-cost) to monitor status.

---

## Contract Version Introspection (`version`)

### Overview

`EscrowContract::version(&env)` is a **pure, read-only** method that returns the
semantic version of the compiled contract WASM binary as a `SorobanString`.

```rust
let env = Env::default();
let version: SorobanString = EscrowContract::version(&env);
assert_eq!(version.to_string(), "1.0.0");
```

### Version semantics

| Segment | Meaning                                                              |
|---------|----------------------------------------------------------------------|
| MAJOR   | Breaking change to the public interface or on-chain storage layout   |
| MINOR   | Backwards-compatible new functionality                               |
| PATCH   | Backwards-compatible bug-fix or documentation change only            |

### Why this matters

- **Tooling & indexers** can call `version()` before any interaction and fail
  fast on an incompatible version range.
- **Migration scripts** must re-read the version after a WASM upgrade to detect
  storage-layout changes (MAJOR bump).
- **Monitoring** can alert when a newly deployed binary carries an unexpected
  version string.

### Security properties

| Property            | Detail                                                                    |
|---------------------|---------------------------------------------------------------------------|
| No state mutation   | Safe to call from any context; cannot trigger side-effects.               |
| No auth required    | Purely informational; any caller may invoke it.                           |
| Tamper-resistant    | Value is a compile-time constant embedded in the WASM binary; it cannot be changed without redeployment. |

### Upgrade workflow

1. Bump `CONTRACT_VERSION` in `escrow/src/lib.rs`.
2. Run `cargo fmt && cargo test` — all tests must pass.
3. Deploy the new WASM binary.
4. Tooling calls `version()` on the live contract to confirm the upgrade.

---

## CI/CD

GitHub Actions runs on every push and pull request to `main`:

- **Format** — `cargo fmt --all -- --check`
- **Build** — `cargo build`
- **Tests** — `cargo test`

Keep formatting and tests passing before opening a PR.

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

We welcome new contracts (e.g. settlement, tokenization helpers), tests, and docs that align with LiquiFact's invoice financing flow.

---

## License

MIT (see root LiquiFact project for full license).