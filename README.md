
## Usage
```bash
cargo run -- transactions.csv > accounts.csv
```

## Running Tests
```bash
cargo test
```

## Design Decisions

### Decimal Precision
Amounts are stored as `rust_decimal::Decimal` rather than `f64`. Floating point
arithmetic is unsuitable for financial systems and will lead to rounding errors
that compound over many transactions. An alternative would be storing values as
integers in their smallest unit (e.g. cents) if the runtime overhead of `rust_decimal` is not deemed acceptable.

### Streaming
The entry point in `main.rs` wraps the input file in a `BufReader`, streaming
transactions row by row rather than loading the entire file into memory.

`process_transactions` is generic over `R: std::io::Read`, meaning the engine can
process transactions from any source like a file, a TCP stream, or an in-memory buffer without any changes to the core logic.

### Memory Bounds
Client accounts and the transaction ledger are held in memory as `HashMap`s.
For this toy engine that is acceptable, but in a production system the transaction
ledger would be backed by a persistent store (e.g. Redis).

### Transaction Ledger
All deposits and withdrawals are stored in a `HashMap<u32, TxRecord>` keyed by
transaction ID. This is required to look up transactions during dispute resolution.
The `TxRecord` tracks the transaction type and status, allowing the engine to
validate state transitions and prevent invalid operations.

### Logging Strategy
Program failures such as unparseable CSV rows, missing amounts, and file not found
are always logged to stderr, these are our errors and should never go unnoticed.

Partner errors such as insufficient funds, invalid dispute states, and wrong client
ownership are silently ignored in production as per instructions. These are
wrapped in `#[cfg(debug_assertions)]` so they are compiled out entirely in release
builds.

In a production grade system, errors would be propagated up the call stack using
distinguished error types, for example via `thiserror` or `anyhow`/`eyre` allowing
the caller to decide whether to retry, skip, or fail. The current approach of
logging and continuing is a deliberate simplification for this toy engine.


### Testing Strategy
Business logic is verified through behavioural tests where each test describes a specific
scenario and asserts the resulting account state. We do not test for type correctness because the Rust compiler enforces these guarantees at
compile time.

## Assumptions

The spec leaves several cases undefined. We make the following assumptions:

**Locked accounts reject all transactions.**
The spec says accounts are frozen after a chargeback but does not define frozen
behaviour. We assume a frozen account is fully immutable so no deposits, withdrawals,
or disputes are accepted.

**Only deposits can be disputed.**
The spec's fraud example specifically describes reversing a deposit. Disputing a
withdrawal has ambiguous semantics in the context of a chargeback, and the spec
does not provide guidance. Disputes referencing withdrawals are silently ignored
to prevent state corruption.

**Disputes that would overdraw available balance are rejected.**
If a client has deposited and partially withdrawn, disputing the original deposit
could push their available balance negative. We reject such disputes. We should
not allow funds to go negative silently.

**Only the owning client can dispute a transaction.**
A dispute referencing a transaction belonging to a different client is silently
ignored. This prevents clients from disputing each other's transactions (bad actor).

