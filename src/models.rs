use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Account {
    pub client: u16,
    pub available: Decimal,
    pub held: Decimal,
    pub locked: bool,
}

impl Account {
    pub fn new(client: u16) -> Self {
        Self {
            client,
            available: Decimal::ZERO,
            held: Decimal::ZERO,
            locked: false,
        }
    }
    
    pub fn total(&self) -> Decimal {
        self.available + self.held
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransactionRow {
    #[serde(rename = "type")]
    pub tx_type: TransactionType,
    pub client: u16,
    pub tx: u32,
    #[serde(default)]
    pub amount: Option<Decimal>,
}

#[derive(Debug)]
pub struct AccountOutput {
    pub client: u16,
    pub available: Decimal,
    pub held: Decimal,
    pub total: Decimal,
    pub locked: bool,
}

impl From<&Account> for AccountOutput {
    fn from(acc: &Account) -> Self {
        Self {
            client: acc.client,
            available: acc.available,
            held: acc.held,
            total: acc.total(),
            locked: acc.locked,
        }
    }
}

impl TransactionRow {
    pub fn tx_type_str(&self) -> &str {
        match self.tx_type {
            TransactionType::Deposit => "deposit",
            TransactionType::Withdrawal => "withdrawal",
            TransactionType::Dispute => "dispute",
            TransactionType::Resolve => "resolve",
            TransactionType::Chargeback => "chargeback",
        }
    }
}

pub fn parse_transaction_type(s: &str) -> Result<TransactionType, anyhow::Error> {
    match s.trim().to_lowercase().as_str() {
        "deposit" => Ok(TransactionType::Deposit),
        "withdrawal" => Ok(TransactionType::Withdrawal),
        "dispute" => Ok(TransactionType::Dispute),
        "resolve" => Ok(TransactionType::Resolve),
        "chargeback" => Ok(TransactionType::Chargeback),
        _ => anyhow::bail!("Unknown transaction type: {}", s),
    }
}
