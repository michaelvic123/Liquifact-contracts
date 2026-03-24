# LiquiFact Contracts

Soroban smart contracts for **LiquiFact** - the global invoice liquidity network on Stellar. This repo contains the **escrow** contract that holds investor funds for tokenized invoices until settlement.

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
   cargo test --features testutils
   ```

---

## Development

| Command | Description |
|---|---|
| `cargo build` | Build all contracts |
| `cargo test --features testutils` | Run unit tests |
| `cargo fmt` | Format code |
| `cargo fmt -- --check` | Check formatting (used in CI) |

---

## Project structure

```
liquifact-contracts/
├── Cargo.toml           # Workspace definition
├── escrow/
│   ├── Cargo.toml       # Escrow contract crate
│   └── src/
│       ├── lib.rs       # LiquiFact escrow contract
│       └── test.rs      # Unit tests (22 tests)
└── .github/workflows/
    └── ci.yml           # CI: fmt, build, test
```

### Escrow contract (high level)

| Method | Description |
|---|---|
| `init(invoice_id, sme_address, amount, yield_bps, maturity)` | Create an invoice escrow. Requires `sme_address` auth. Panics if already initialised. |
| `get_escrow()` | Read current escrow state. |
| `fund(investor, amount)` | Record investor funding. Requires `investor` auth. Accumulates the investor's contribution in the per-investor ledger. Status becomes `funded` (1) when `funded_amount >= funding_target`. |
| `get_contribution(investor)` | Return the cumulative amount contributed by `investor` (0 if never funded). |
| `settle()` | Mark escrow as settled. Requires `sme_address` auth and `ledger.timestamp >= maturity` (unless `maturity == 0`). |

#### Per-investor contribution ledger

Each `fund` call stores the investor's running total under a typed
`DataKey::InvestorContribution(Address)` key in instance storage. This enables:

- **Payout accounting** - calculate each investor's share of principal + yield without replaying history.
- **Auditability** - any party can call `get_contribution(investor)` to verify on-chain how much a given address contributed.
- **Future partial settlement** - the ledger is the foundation for pro-rata release logic once partial settlement is supported.

#### Security notes

- `init` requires the SME address to authorise the call, preventing anyone from hijacking a contract instance with a different SME wallet.
- `fund` requires the investor to authorise the call, preventing third-party spoofing of contributions.
- `settle` requires the SME address to authorise and enforces the maturity timestamp, preventing premature settlement.
- Repeated funding by the same investor is explicitly supported and accumulates correctly; the ledger entry is additive, not overwritten.
- All storage uses the typed `DataKey` enum - raw symbol collisions between the singleton escrow key and per-investor keys are impossible by construction.

---

## CI/CD

GitHub Actions runs on every push and pull request to `main`:

- **Format** - `cargo fmt --all -- --check`
- **Build** - `cargo build`
- **Tests** - `cargo test`

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
   - `cargo test --features testutils`
6. **Commit** with clear messages (e.g. `feat(escrow): X`, `test(escrow): Y`).
7. **Push** to your fork and open a **Pull Request** to `main`.
8. Wait for CI and address review feedback.

We welcome new contracts (e.g. settlement, tokenization helpers), tests, and docs that align with LiquiFact's invoice financing flow.

---

## License

MIT (see root LiquiFact project for full license).
