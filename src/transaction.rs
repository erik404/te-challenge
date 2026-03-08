use rust_decimal::Decimal;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::str::FromStr;

/// In-memory ledger of processed transactions for look-up during dispute, resolve, and chargeback. Keyed by tx_id
pub type TxLedger = HashMap<u32, TxRecord>;

#[derive(Debug)]
pub struct TxRecord {
    client: u16,
    amount: Decimal,
    status: TxStatus,
    tx_type: TxType,
}

#[derive(Debug)]
pub enum TxStatus {
    Normal,
    Disputed,
    ChargedBack,
}

impl TxRecord {
    pub fn status(&self) -> &TxStatus {
        &self.status
    }
    pub fn amount(&self) -> Decimal {
        self.amount
    }
    pub fn client(&self) -> u16 {
        self.client
    }
    pub fn set_status(&mut self, status: TxStatus) {
        self.status = status;
    }
    pub fn tx_type(&self) -> &TxType {
        &self.tx_type
    }
}

impl From<&Transaction> for TxRecord {
    fn from(tx: &Transaction) -> Self {
        Self {
            client: tx.client(),
            amount: tx.amount().unwrap_or(Decimal::ZERO),
            status: TxStatus::Normal,
            tx_type: *tx.tx_type(),
        }
    }
}

/// Represents a single transaction row from the CSV input
#[derive(Debug, Deserialize)]
pub struct Transaction {
    #[serde(rename = "type", deserialize_with = "deserialize_tx_type")]
    tx_type: TxType,
    client: u16,
    tx: u32,
    amount: Option<Decimal>,
}

#[derive(Debug, Clone, Copy)]
pub enum TxType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

impl Transaction {
    pub fn amount(&self) -> Option<Decimal> {
        self.amount
    }
    pub fn tx_type(&self) -> &TxType {
        &self.tx_type
    }
    pub fn client(&self) -> u16 {
        self.client
    }
    pub fn tx_id(&self) -> u32 {
        self.tx
    }
}

fn deserialize_tx_type<'de, D: Deserializer<'de>>(d: D) -> Result<TxType, D::Error> {
    let s = String::deserialize(d)?;
    TxType::from_str(&s).map_err(D::Error::custom)
}

impl FromStr for TxType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "deposit" => Ok(TxType::Deposit),
            "withdrawal" => Ok(TxType::Withdrawal),
            "dispute" => Ok(TxType::Dispute),
            "resolve" => Ok(TxType::Resolve),
            "chargeback" => Ok(TxType::Chargeback),
            _ => Err(format!("Unknown transaction type: {}", s)),
        }
    }
}
