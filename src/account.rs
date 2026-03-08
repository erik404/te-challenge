use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;

/// In-memory store of client accounts, keyed by client id
pub type Accounts = HashMap<u16, Account>;

#[derive(Debug)]
pub struct Account {
    available: Decimal,
    held: Decimal,
    locked: bool,
}

impl Account {
    pub fn new() -> Self {
        Account {
            available: Decimal::ZERO,
            held: Decimal::ZERO,
            locked: false,
        }
    }
    pub fn available(&self) -> Decimal {
        self.available
    }
    pub fn held(&self) -> Decimal {
        self.held
    }
    pub fn total(&self) -> Decimal {
        self.available + self.held
    }
    pub fn locked(&self) -> bool {
        self.locked
    }
    pub fn credit(&mut self, amount: Decimal) {
        self.available += amount;
    }
    pub fn debit(&mut self, amount: Decimal) {
        self.available -= amount;
    }
    pub fn hold(&mut self, amount: Decimal) {
        self.held += amount;
    }
    pub fn release(&mut self, amount: Decimal) {
        self.held -= amount;
    }
    pub fn chargeback(&mut self, amount: Decimal) {
        self.held -= amount;
        self.locked = true;
    }
}

/// Serializable representation of a client account for CSV output
#[derive(Serialize)]
pub struct AccountOutput {
    client: u16,
    available: Decimal,
    held: Decimal,
    total: Decimal,
    locked: bool,
}

impl AccountOutput {
    pub fn new(client: u16, account: &Account) -> Self {
        Self {
            client,
            available: account.available().round_dp(4),
            held: account.held().round_dp(4),
            total: account.total().round_dp(4),
            locked: account.locked(),
        }
    }
}
