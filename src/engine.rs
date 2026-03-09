use crate::account::{Account, AccountOutput, Accounts};
use crate::transaction::{Transaction, TxLedger, TxRecord, TxStatus, TxType};
use std::collections::HashMap;
use std::io::Read;

/// Processes a CSV stream of transactions and returns the final account states
/// Accepts files, TCP streams and in-memory
pub fn process_transactions<R: Read>(reader: R) -> Accounts {
    let mut csv_reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(reader);

    let mut accounts: Accounts = HashMap::new();
    let mut tx_ledger: TxLedger = HashMap::new();

    for result in csv_reader.deserialize::<Transaction>() {
        match result {
            Ok(tx) => process_transaction(tx, &mut accounts, &mut tx_ledger),
            Err(e) => eprintln!("Skipping invalid row: {}", e),
        }
    }

    accounts
}

/// Serializes account states as CSV to stdout
pub fn output_statement(accounts: &Accounts) {
    let mut writer = csv::Writer::from_writer(std::io::stdout());

    for (client_id, account) in accounts {
        let record = AccountOutput::new(*client_id, account);
        if let Err(e) = writer.serialize(record) {
            eprintln!("Failed to serialize account {}: {}", client_id, e);
        }
    }

    if let Err(e) = writer.flush() {
        eprintln!("Failed to flush output: {}", e);
    }
}

fn process_transaction(tx: Transaction, accounts: &mut Accounts, tx_ledger: &mut TxLedger) {
    let account = accounts.entry(tx.client()).or_insert_with(Account::new);

    if account.locked() {
        #[cfg(debug_assertions)]
        eprintln!(
            "Account {} is locked, skipping tx {}",
            tx.client(),
            tx.tx_id()
        );
        return;
    }

    match tx.tx_type() {
        TxType::Deposit => process_deposit(&tx, account, tx_ledger),
        TxType::Withdrawal => process_withdrawal(&tx, account, tx_ledger),
        TxType::Dispute => process_dispute(&tx, account, tx_ledger),
        TxType::Resolve => process_resolve(&tx, account, tx_ledger),
        TxType::Chargeback => process_chargeback(&tx, account, tx_ledger),
    }
}

fn process_deposit(tx: &Transaction, account: &mut Account, tx_ledger: &mut TxLedger) {
    let Some(amount) = tx.amount() else {
        eprintln!("Deposit tx {} missing amount, skipping", tx.tx_id());
        return;
    };

    account.credit(amount);
    tx_ledger.insert(tx.tx_id(), TxRecord::from(tx));
}

fn process_withdrawal(tx: &Transaction, account: &mut Account, tx_ledger: &mut TxLedger) {
    let Some(amount) = tx.amount() else {
        eprintln!("Withdrawal tx {} missing amount, skipping", tx.tx_id());
        return;
    };

    if account.available() < amount {
        #[cfg(debug_assertions)]
        eprintln!("Insufficient funds for tx {}, skipping", tx.tx_id());
        return;
    }

    account.debit(amount);
    tx_ledger.insert(tx.tx_id(), TxRecord::from(tx));
}

fn process_dispute(tx: &Transaction, account: &mut Account, tx_ledger: &mut TxLedger) {
    let Some(tx_record) = tx_ledger.get_mut(&tx.tx_id()) else {
        #[cfg(debug_assertions)]
        eprintln!("Dispute tx {} not found, skipping", tx.tx_id());
        return;
    };

    if !verify_tx_ownership(tx_record, tx) {
        return;
    }

    if !matches!(tx_record.status(), TxStatus::Normal) {
        #[cfg(debug_assertions)]
        eprintln!("Tx {} invalid state for dispute, skipping", tx.tx_id());
        return;
    }

    if !matches!(tx_record.tx_type(), TxType::Deposit) {
        #[cfg(debug_assertions)]
        eprintln!(
            "Tx {} is not a deposit, cannot be disputed, skipping",
            tx.tx_id()
        );
        return;
    }

    if account.available() < tx_record.amount() {
        #[cfg(debug_assertions)]
        eprintln!("Dispute tx {} would overdraw account, skipping", tx.tx_id());
        return;
    }

    account.debit(tx_record.amount());
    account.hold(tx_record.amount());
    tx_record.set_status(TxStatus::Disputed);
}

fn process_resolve(tx: &Transaction, account: &mut Account, tx_ledger: &mut TxLedger) {
    let Some(tx_record) = tx_ledger.get_mut(&tx.tx_id()) else {
        #[cfg(debug_assertions)]
        eprintln!("Resolve tx {} not found, skipping", tx.tx_id());
        return;
    };

    if !verify_tx_ownership(tx_record, tx) {
        return;
    }

    if !matches!(tx_record.status(), TxStatus::Disputed) {
        #[cfg(debug_assertions)]
        eprintln!("Tx {} invalid state for resolve, skipping", tx.tx_id());
        return;
    }

    account.release(tx_record.amount());
    account.credit(tx_record.amount());
    tx_record.set_status(TxStatus::Normal);
}

fn process_chargeback(tx: &Transaction, account: &mut Account, tx_ledger: &mut TxLedger) {
    let Some(tx_record) = tx_ledger.get_mut(&tx.tx_id()) else {
        #[cfg(debug_assertions)]
        eprintln!("Chargeback tx {} not found, skipping", tx.tx_id());
        return;
    };

    if !verify_tx_ownership(tx_record, tx) {
        return;
    }

    if !matches!(tx_record.status(), TxStatus::Disputed) {
        #[cfg(debug_assertions)]
        eprintln!("Tx {} invalid state for chargeback, skipping", tx.tx_id());
        return;
    }

    account.chargeback(tx_record.amount());
    tx_record.set_status(TxStatus::ChargedBack);
}

fn verify_tx_ownership(tx_record: &TxRecord, tx: &Transaction) -> bool {
    if tx_record.client() != tx.client() {
        #[cfg(debug_assertions)]
        eprintln!(
            "Tx {} does not belong to client {}, skipping",
            tx.tx_id(),
            tx.client()
        );
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    // Verifies that a deposit correctly increases the client's available balance and that held balance remains zero
    #[test]
    fn test_deposit_increases_available() {
        let accounts = process_transactions("type,client,tx,amount\ndeposit,1,1,100.0".as_bytes());
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::new(100_0000, 4));
        assert_eq!(account.held(), Decimal::ZERO);
        assert_eq!(account.total(), Decimal::new(100_0000, 4));
        assert!(!account.locked());
    }

    // Verifies that a withdrawal correctly decreases the client's available balance
    #[test]
    fn test_withdrawal_decreases_available() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         withdrawal,1,2,40.0"
                .as_bytes(),
        );
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::new(60_0000, 4));
        assert_eq!(account.held(), Decimal::ZERO);
        assert_eq!(account.total(), Decimal::new(60_0000, 4));
        assert!(!account.locked());
    }

    // Verifies that a withdrawal exceeding available balance is rejected silently leaving the account unchanged
    #[test]
    fn test_withdrawal_fails_insufficient_funds() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         withdrawal,1,2,200.0"
                .as_bytes(),
        );
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::new(100_0000, 4));
        assert_eq!(account.held(), Decimal::ZERO);
        assert_eq!(account.total(), Decimal::new(100_0000, 4));
        assert!(!account.locked());
    }

    // Verifies that a dispute moves funds from available to held keeping total unchanged
    #[test]
    fn test_dispute_moves_funds_to_held() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         dispute,1,1,"
                .as_bytes(),
        );
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::ZERO);
        assert_eq!(account.held(), Decimal::new(100_0000, 4));
        assert_eq!(account.total(), Decimal::new(100_0000, 4));
        assert!(!account.locked());
    }

    // Verifies that resolving a dispute releases held funds back to available keeping total unchanged
    #[test]
    fn test_resolve_releases_held_funds() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         dispute,1,1,\n\
         resolve,1,1,"
                .as_bytes(),
        );
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::new(100_0000, 4));
        assert_eq!(account.held(), Decimal::ZERO);
        assert_eq!(account.total(), Decimal::new(100_0000, 4));
        assert!(!account.locked());
    }

    // Verifies that a chargeback removes held funds entirely and locks the account
    #[test]
    fn test_chargeback_locks_account() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         dispute,1,1,\n\
         chargeback,1,1,"
                .as_bytes(),
        );
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::ZERO);
        assert_eq!(account.held(), Decimal::ZERO);
        assert_eq!(account.total(), Decimal::ZERO);
        assert!(account.locked());
    }

    // Verifies that a locked account rejects all further transactions.
    #[test]
    fn test_locked_account_rejects_transactions() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         dispute,1,1,\n\
         chargeback,1,1,\n\
         deposit,1,2,500.0\n\
         withdrawal,1,3,50.0"
                .as_bytes(),
        );
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::ZERO);
        assert_eq!(account.held(), Decimal::ZERO);
        assert_eq!(account.total(), Decimal::ZERO);
        assert!(account.locked());
    }

    // Verifies that a client cannot dispute a transaction belonging to another client
    #[test]
    fn test_dispute_wrong_client_ignored() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         deposit,2,2,200.0\n\
         dispute,2,1,"
                .as_bytes(),
        );

        let account1 = accounts.get(&1).unwrap();
        assert_eq!(account1.available(), Decimal::new(100_0000, 4));
        assert_eq!(account1.held(), Decimal::ZERO);
        assert_eq!(account1.total(), Decimal::new(100_0000, 4));
        assert!(!account1.locked());

        let account2 = accounts.get(&2).unwrap();
        assert_eq!(account2.available(), Decimal::new(200_0000, 4));
        assert_eq!(account2.held(), Decimal::ZERO);
        assert_eq!(account2.total(), Decimal::new(200_0000, 4));
        assert!(!account2.locked());
    }

    // Verifies that a resolve on a transaction that was never disputed is ignored
    #[test]
    fn test_resolve_without_dispute() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         resolve,1,1,"
                .as_bytes(),
        );
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::new(100_0000, 4));
        assert_eq!(account.held(), Decimal::ZERO);
        assert_eq!(account.total(), Decimal::new(100_0000, 4));
        assert!(!account.locked());
    }

    // Verifies that a dispute is rejected if it would push available balance negative.
    #[test]
    fn test_prevent_overdraw_dispute() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         withdrawal,1,2,90.0\n\
         dispute,1,1,"
                .as_bytes(),
        );
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::new(10_0000, 4));
        assert_eq!(account.held(), Decimal::ZERO);
        assert_eq!(account.total(), Decimal::new(10_0000, 4));
        assert!(!account.locked());
    }

    // Verifies that operations on one client account do not affect other clients
    #[test]
    fn test_multiple_clients_independent() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         deposit,2,2,200.0\n\
         dispute,1,1,\n\
         chargeback,1,1,"
                .as_bytes(),
        );

        let account1 = accounts.get(&1).unwrap();
        assert_eq!(account1.available(), Decimal::ZERO);
        assert_eq!(account1.held(), Decimal::ZERO);
        assert_eq!(account1.total(), Decimal::ZERO);
        assert!(account1.locked());

        let account2 = accounts.get(&2).unwrap();
        assert_eq!(account2.available(), Decimal::new(200_0000, 4));
        assert_eq!(account2.held(), Decimal::ZERO);
        assert_eq!(account2.total(), Decimal::new(200_0000, 4));
        assert!(!account2.locked());
    }

    // Verifies that a withdrawal of exactly the available balance succeeds leaving the account at zero without locking it
    #[test]
    fn test_withdrawal_exact_balance() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         withdrawal,1,2,100.0"
                .as_bytes(),
        );
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::ZERO);
        assert_eq!(account.held(), Decimal::ZERO);
        assert_eq!(account.total(), Decimal::ZERO);
        assert!(!account.locked());
    }

    // Verifies that a withdrawal cannot be disputed.
    #[test]
    fn test_withdrawal_cannot_be_disputed() {
        let accounts = process_transactions(
            "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         withdrawal,1,2,40.0\n\
         dispute,1,2,"
                .as_bytes(),
        );
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::new(60_0000, 4));
        assert_eq!(account.held(), Decimal::ZERO);
        assert_eq!(account.total(), Decimal::new(60_0000, 4));
        assert!(!account.locked());
    }

    // Verifies that whitespace is handled correctly
    #[test]
    fn test_whitespace_trimming() {
        let accounts = process_transactions(
            "type, client  , tx, amount\ndeposit, 1, 1  , 100.0".as_bytes()
        );
        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available(), Decimal::new(100_0000, 4));
        assert_eq!(account.total(), Decimal::new(100_0000, 4));
        assert!(!account.locked());
    }
}
