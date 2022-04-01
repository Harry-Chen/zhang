use crate::core::account::Account;
use crate::core::amount::Amount;
use crate::core::data::{Balance, BalanceCheck, BalancePad, Date, Transaction, TxnPosting};
use crate::core::inventory::AccountName;
use crate::core::ledger::{AccountInfo, AccountStatus, CurrencyInfo, DocumentType, Inventory, LedgerError};
use crate::core::models::Directive;
use crate::server::LedgerState;
use async_graphql::{Context, Interface, Object};
use chrono::{NaiveDate, NaiveDateTime, Utc};
use itertools::Itertools;
use std::collections::HashMap;
use std::ops::Sub;
use std::path::PathBuf;

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn entries(&self, ctx: &Context<'_>) -> Vec<FileEntryDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage
            .visited_files
            .iter()
            .map(|it| FileEntryDto(it.clone()))
            .collect_vec()
    }
    async fn entry(&self, ctx: &Context<'_>, name: String) -> Option<FileEntryDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage
            .visited_files
            .iter()
            .find(|it| it.to_str().map(|path_str| name.eq(path_str)).unwrap_or(false))
            .map(|it| FileEntryDto(it.clone()))
    }
    async fn statistic(&self, ctx: &Context<'_>, from: i64, to: i64) -> StatisticDto {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        let start_date = NaiveDateTime::from_timestamp(from, 0).date();
        let end_date = NaiveDateTime::from_timestamp(to, 0).date();
        let start_date_snapshot = ledger_stage.daily_inventory.get_account_inventory(&start_date);
        let end_date_snapshot = ledger_stage.daily_inventory.get_account_inventory(&end_date);
        StatisticDto {
            start_date,
            end_date,
            start_snapshot: start_date_snapshot,
            ens_snapshot: end_date_snapshot,
        }
    }
    async fn currencies(&self, ctx: &Context<'_>) -> Vec<CurrencyDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage
            .currencies
            .clone()
            .into_iter()
            .map(|(_, info)| CurrencyDto(info))
            .collect_vec()
    }
    async fn currency(&self, ctx: &Context<'_>, name: String) -> Option<CurrencyDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage.currencies.get(&name).map(|info| CurrencyDto(info.clone()))
    }

    async fn accounts(&self, ctx: &Context<'_>) -> Vec<AccountDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage
            .accounts
            .clone()
            .into_iter()
            .map(|(name, info)| AccountDto { name, info })
            .collect_vec()
    }
    async fn account(&self, ctx: &Context<'_>, name: String) -> Option<AccountDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage
            .accounts
            .get(&name)
            .cloned()
            .map(|info| AccountDto { name, info })
    }

    async fn documents(&self, ctx: &Context<'_>) -> Vec<DocumentDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage
            .documents
            .values()
            .cloned()
            .map(|it| match it {
                DocumentType::AccountDocument {
                    date,
                    account,
                    filename,
                } => DocumentDto::AccountDocument(AccountDocumentDto {
                    date,
                    account,
                    filename,
                }),
                DocumentType::TransactionDocument { .. } => DocumentDto::TransactionDocument(TransactionDocumentDto {}),
            })
            .collect_vec()
    }

    async fn journals(&self, ctx: &Context<'_>) -> Vec<JournalDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage
            .directives
            .iter()
            .filter_map(|directive| match directive {
                Directive::Transaction(trx) => Some(JournalDto::Transaction(TransactionDto(trx.clone()))),
                Directive::Balance(balance) => match balance {
                    Balance::BalanceCheck(check) => Some(JournalDto::BalanceCheck(BalanceCheckDto(check.clone()))),
                    Balance::BalancePad(pad) => Some(JournalDto::BalancePad(BalancePadDto(pad.clone()))),
                },
                _ => None,
            })
            .rev()
            .collect_vec()
    }

    async fn errors(&self, ctx: &Context<'_>) -> Vec<ErrorDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage.errors.iter().cloned().map(ErrorDto).collect_vec()
    }
}

pub struct AccountDto {
    name: String,
    info: AccountInfo,
}

#[Object]
impl AccountDto {
    async fn name(&self) -> String {
        self.name.to_string()
    }
    async fn status(&self) -> AccountStatus {
        self.info.status
    }
    async fn snapshot(&self, ctx: &Context<'_>) -> SnapshotDto {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        let snapshot = ledger_stage
            .account_inventory
            .iter()
            .filter(|(ac, _)| ac.as_str().eq(&self.name))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        SnapshotDto {
            date: Utc::now().naive_local(),
            account_inventory: snapshot,
        }
    }
    async fn currencies(&self, ctx: &Context<'_>) -> Vec<CurrencyDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage
            .currencies
            .clone()
            .into_iter()
            .filter(|(name, _)| self.info.currencies.contains(name))
            .map(|(_, info)| CurrencyDto(info))
            .collect_vec()
    }
    async fn journals(&self, ctx: &Context<'_>) -> Vec<JournalDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage
            .directives
            .iter()
            .filter(|directive| match directive {
                Directive::Transaction(trx) => trx.has_account(&self.name),
                Directive::Balance(balance) => match balance {
                    Balance::BalanceCheck(check) => check.account.content.eq(&self.name),
                    Balance::BalancePad(pad) => pad.account.content.eq(&self.name),
                },
                _ => false,
            })
            .filter_map(|directive| match directive {
                Directive::Transaction(trx) => Some(JournalDto::Transaction(TransactionDto(trx.clone()))),
                Directive::Balance(balance) => match balance {
                    Balance::BalanceCheck(check) => Some(JournalDto::BalanceCheck(BalanceCheckDto(check.clone()))),
                    Balance::BalancePad(pad) => Some(JournalDto::BalancePad(BalancePadDto(pad.clone()))),
                },
                _ => None,
            })
            .rev()
            .collect_vec()
    }

    async fn documents(&self, ctx: &Context<'_>) -> Vec<DocumentDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage
            .documents
            .values()
            .filter(|it| match it {
                DocumentType::AccountDocument { account, .. } => account.content.eq(&self.name),
                DocumentType::TransactionDocument { .. } => false, // todo transaction documents
            })
            .cloned()
            .map(|it| match it {
                DocumentType::AccountDocument {
                    date,
                    account,
                    filename,
                } => DocumentDto::AccountDocument(AccountDocumentDto {
                    date,
                    account,
                    filename,
                }),
                DocumentType::TransactionDocument { .. } => DocumentDto::TransactionDocument(TransactionDocumentDto {}),
            })
            .collect_vec()
    }
    async fn one_meta(&self, key: String) -> Option<String> {
        self.info.meta.get_one(&key).cloned()
    }
    async fn meta(&self, key: String) -> Vec<String> {
        self.info.meta.get_all(&key).into_iter().cloned().collect_vec()
    }
}

pub struct CurrencyDto(CurrencyInfo);

#[Object]
impl CurrencyDto {
    async fn name(&self) -> String {
        self.0.commodity.currency.to_string()
    }

    async fn precision(&self) -> i32 {
        self.0
            .commodity
            .meta
            .get("precision")
            .map(|it| it.clone().to_plain_string())
            .map(|it| it.parse::<i32>().unwrap_or(2))
            .unwrap_or(2)
    }
}

#[derive(Interface)]
#[graphql(field(name = "date", type = "String"))]
pub enum JournalDto {
    Transaction(TransactionDto),
    BalanceCheck(BalanceCheckDto),
    BalancePad(BalancePadDto),
}

pub struct TransactionDto(Transaction);

#[Object]
impl TransactionDto {
    async fn date(&self) -> String {
        self.0.date.naive_date().to_string()
    }
    async fn payee(&self) -> Option<String> {
        self.0.payee.clone().map(|it| it.to_plain_string())
    }
    async fn narration(&self) -> Option<String> {
        self.0.narration.clone().map(|it| it.to_plain_string())
    }
    async fn postings<'a>(&'a self) -> Vec<PostingDto<'a>> {
        self.0.txn_postings().into_iter().map(PostingDto).collect_vec()
    }
}

pub struct BalanceCheckDto(BalanceCheck);

#[Object]
impl BalanceCheckDto {
    async fn date(&self) -> String {
        self.0.date.naive_date().to_string()
    }
    async fn account(&self, ctx: &Context<'_>) -> Option<AccountDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage.accounts.get(self.0.account.name()).map(|info| AccountDto {
            name: self.0.account.name().to_string(),
            info: info.clone(),
        })
    }
    async fn balance_amount(&self) -> AmountDto {
        AmountDto(self.0.amount.clone())
    }
    async fn current_amount(&self) -> AmountDto {
        AmountDto(self.0.current_amount.clone().expect("cannot get current amount"))
    }
    async fn distance(&self) -> Option<AmountDto> {
        self.0.distance.clone().map(AmountDto)
    }
    async fn is_balanced(&self) -> bool {
        self.0.distance.is_none()
    }
}

pub struct BalancePadDto(BalancePad);

#[Object]
impl BalancePadDto {
    async fn date(&self) -> String {
        self.0.date.naive_date().to_string()
    }
}

pub struct PostingDto<'a>(TxnPosting<'a>);
#[Object]
impl<'a> PostingDto<'a> {
    async fn account(&self, ctx: &Context<'_>) -> Option<AccountDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        ledger_stage
            .accounts
            .get(self.0.posting.account.name())
            .map(|info| AccountDto {
                name: self.0.posting.account.name().to_string(),
                info: info.clone(),
            })
    }

    async fn unit(&self) -> AmountDto {
        AmountDto(self.0.units())
    }
}
pub struct AmountDto(Amount);

#[Object]
impl AmountDto {
    async fn number(&self) -> String {
        self.0.number.to_string()
    }
    async fn currency(&self) -> String {
        self.0.currency.clone()
    }
}

pub struct StatisticDto {
    start_date: NaiveDate,
    start_snapshot: HashMap<AccountName, Inventory>,

    end_date: NaiveDate,
    ens_snapshot: HashMap<AccountName, Inventory>,
}

#[Object]
impl StatisticDto {
    async fn start(&self) -> i64 {
        self.start_date.and_hms(0, 0, 0).timestamp()
    }
    async fn end(&self) -> i64 {
        self.end_date.and_hms(0, 0, 0).timestamp()
    }
    async fn accounts(&self) -> Vec<AccountDto> {
        // todo
        vec![]
    }
    async fn category_snapshot(&self, categories: Vec<String>) -> SnapshotDto {
        let dto = self
            .ens_snapshot
            .iter()
            .filter(|(account_name, _)| categories.iter().any(|category| account_name.starts_with(category)))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        SnapshotDto {
            date: self.end_date.and_hms(0, 0, 0),
            account_inventory: dto,
        }
    }
    async fn distance(&self, ctx: &Context<'_>, #[graphql(default)] accounts: Vec<String>) -> SnapshotDto {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;

        let account_filter = |&(k, _v): &(&AccountName, &Inventory)| {
            if accounts.is_empty() {
                true
            } else {
                let x1 = k.as_str();
                accounts.iter().any(|it| it.eq(x1))
            }
        };
        let mut ret: HashMap<AccountName, Inventory> = self
            .ens_snapshot
            .iter()
            .filter(account_filter)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (account_name, inventory) in self.start_snapshot.iter().filter(account_filter) {
            let target_account_inventory = ret
                .entry(account_name.clone())
                .or_insert_with(|| ledger_stage.default_account_inventory());
            let x = (target_account_inventory as &Inventory).sub(inventory);
            *target_account_inventory = x;
        }
        SnapshotDto {
            date: self.end_date.and_hms(0, 0, 0),
            account_inventory: ret,
        }
    }
}

pub struct SnapshotDto {
    date: NaiveDateTime,
    account_inventory: HashMap<AccountName, Inventory>,
}

#[Object]
impl SnapshotDto {
    async fn summary(&self, ctx: &Context<'_>) -> AmountDto {
        let operating_currency = {
            let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
            ledger_stage
                .option("operating_currency")
                .unwrap_or_else(|| "CNY".to_string())
        };
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;

        let inventory = self
            .account_inventory
            .iter()
            .fold(ledger_stage.default_account_inventory(), |fold, lo| &fold + lo.1);

        let decimal = inventory.calculate_to_currency(self.date, &operating_currency);
        AmountDto(Amount::new(decimal, operating_currency))
    }
    async fn detail(&self, ctx: &Context<'_>) -> Vec<AmountDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        let inventory = self
            .account_inventory
            .iter()
            .fold(ledger_stage.default_account_inventory(), |fold, lo| &fold + lo.1);

        inventory
            .inner
            .into_iter()
            .map(|(c, n)| Amount::new(n, c))
            .map(AmountDto)
            .collect_vec()
    }
}

pub struct FileEntryDto(PathBuf);

#[Object]
impl FileEntryDto {
    async fn name(&self) -> Option<&str> {
        self.0.to_str()
    }
    async fn content(&self) -> String {
        std::fs::read_to_string(&self.0).expect("Cannot open file")
    }
}

#[derive(Interface)]
#[graphql(field(name = "filename", type = "String"))]
pub enum DocumentDto {
    AccountDocument(AccountDocumentDto),
    TransactionDocument(TransactionDocumentDto),
}
pub struct AccountDocumentDto {
    date: Date,
    account: Account,
    filename: String,
}

#[Object]
impl AccountDocumentDto {
    async fn date(&self) -> i64 {
        self.date.naive_datetime().timestamp()
    }
    async fn filename(&self) -> String {
        self.filename.clone()
    }
    async fn account(&self, ctx: &Context<'_>) -> Option<AccountDto> {
        let ledger_stage = ctx.data_unchecked::<LedgerState>().read().await;
        let account_name = self.account.name().to_string();
        ledger_stage
            .accounts
            .get(&account_name)
            .cloned()
            .map(|info| AccountDto {
                name: account_name,
                info,
            })
    }
}

pub struct TransactionDocumentDto {}

#[Object]
impl TransactionDocumentDto {
    async fn filename(&self) -> String {
        "".to_string()
    }
}

pub struct ErrorDto(LedgerError);

#[Object]
impl ErrorDto {
    async fn message(&self) -> String {
        match self.0 {
            LedgerError::AccountBalanceCheckError { .. } => "account not balance".to_string(),
            // LedgerError::AccountDoesNotExist { .. } => "account does not exist".to_string(),
            // LedgerError::AccountClosed { .. } => "account close".to_string(),
            // LedgerError::TransactionDoesNotBalance { .. } => "trx does not balance".to_string(),
        }
    }
}
