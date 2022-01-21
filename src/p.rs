use std::collections::HashMap;
use std::str::FromStr;

use bigdecimal::BigDecimal;
use chrono::{NaiveDate, NaiveDateTime};
use pest_consume::{match_nodes, Error, Parser};
use snailquote::unescape;

use crate::core::account::{Account, AccountType};
use crate::core::amount::Amount;
use crate::core::data::{
    Balance, Close, Commodity, Custom, Date, Document, Event, Note, Open, Pad, Posting, Price,
    Transaction,
};
use crate::core::models::{AvaroString, Directive, Flag, SingleTotalPrice, StringOrAccount};

type Result<T> = std::result::Result<T, Error<Rule>>;
type Node<'i> = pest_consume::Node<'i, Rule, ()>;

#[derive(Parser)]
#[grammar = "avaro.pest"]
pub struct AvaroParser;

#[pest_consume::parser]
impl AvaroParser {
    fn EOI(_input: Node) -> Result<()> {
        Ok(())
    }
    fn number(input: Node) -> Result<BigDecimal> {
        Ok(BigDecimal::from_str(input.as_str()).unwrap())
    }
    fn inner(input: Node) -> Result<String> {
        Ok(input.as_str().to_owned())
    }
    fn quote_string(input: Node) -> Result<AvaroString> {
        let string = input.as_str();
        Ok(AvaroString::QuoteString(unescape(string).unwrap()))
    }

    fn unquote_string(input: Node) -> Result<AvaroString> {
        Ok(AvaroString::UnquoteString(input.as_str().to_owned()))
    }

    fn string(input: Node) -> Result<AvaroString> {
        let ret = match_nodes!(
            input.into_children();
            [quote_string(i)] => i,
            [unquote_string(i)] => i
        );
        Ok(ret)
    }
    fn commodity_name(input: Node) -> Result<String> {
        Ok(input.as_str().to_owned())
    }
    fn account_type(input: Node) -> Result<String> {
        Ok(input.as_str().to_owned())
    }
    fn account_name(input: Node) -> Result<Account> {
        let r: (String, Vec<String>) = match_nodes!(input.into_children();
            [account_type(a), unquote_string(i)..] => {
                (a, i.map(|it|it.to_plain_string()).collect())
            },

        );
        Ok(Account {
            account_type: AccountType::from_str(&r.0).unwrap(),
            content: format!("{}:{}", &r.0, r.1.join(":")),
            components: r.1,
        })
    }
    fn date(input: Node) -> Result<Date> {
        let datetime: Date = match_nodes!(input.into_children();
            [date_only(d)] => d,
            [datetime(d)] => d
        );
        Ok(datetime)
    }

    fn date_only(input: Node) -> Result<Date> {
        let date = NaiveDate::parse_from_str(input.as_str(), "%Y-%m-%d").unwrap();
        Ok(Date::Date(date))
    }
    fn datetime(input: Node) -> Result<Date> {
        Ok(Date::Datetime(
            NaiveDateTime::parse_from_str(input.as_str(), "%Y-%m-%d %H:%M:%S").unwrap(),
        ))
    }

    fn plugin(input: Node) -> Result<Directive> {
        let ret: (AvaroString, Vec<AvaroString>) = match_nodes!(input.into_children();
            [string(module), string(values)..] => (module, values.collect()),
        );
        Ok(Directive::Plugin {
            module: ret.0,
            value: ret.1,
        })
    }

    fn option(input: Node) -> Result<Directive> {
        let ret = match_nodes!(input.into_children();
            [string(key), string(value)] => Directive::Option {key:key,value:value},
        );
        Ok(ret)
    }
    fn comment(input: Node) -> Result<Directive> {
        Ok(Directive::Comment {
            content: input.as_str().to_owned(),
        })
    }

    fn open(input: Node) -> Result<Directive> {
        let ret: (Date, Account, Vec<String>, Vec<(String, AvaroString)>) = match_nodes!(input.into_children();
            [date(date), account_name(a), commodity_name(commodities).., commodity_meta(metas)] => (date, a, commodities.collect(), metas),
            [date(date), account_name(a), commodity_name(commodities)..] => (date, a, commodities.collect(), vec![]),
            [date(date), account_name(a), commodity_meta(metas)] => (date, a, vec![], metas),
        );

        let open = Open {
            date: ret.0,
            account: ret.1,
            commodities: ret.2,
            meta: ret.3.into_iter().collect(),
        };
        Ok(Directive::Open(open))
    }
    fn close(input: Node) -> Result<Directive> {
        let ret: (Date, Account) = match_nodes!(input.into_children();
            [date(date), account_name(a)] => (date, a)
        );
        Ok(Directive::Close(Close {
            date: ret.0,
            account: ret.1,
            meta: Default::default(),
        }))
    }

    fn identation(input: Node) -> Result<()> {
        Ok(())
    }

    fn commodity_line(input: Node) -> Result<(String, AvaroString)> {
        let ret: (String, AvaroString) = match_nodes!(input.into_children();
            [string(key), string(value)] => (key.to_plain_string(), value),
        );
        Ok(ret)
    }

    fn commodity_meta(input: Node) -> Result<Vec<(String, AvaroString)>> {
        let ret: Vec<(String, AvaroString)> = match_nodes!(input.into_children();
            [commodity_line(lines)..] => lines.collect(),
        );
        Ok(ret)
    }

    fn posting_unit(
        input: Node,
    ) -> Result<(
        Amount,
        Option<(Option<Amount>, Option<Date>, Option<SingleTotalPrice>)>,
    )> {
        let ret: (
            Amount,
            Option<(Option<Amount>, Option<Date>, Option<SingleTotalPrice>)>,
        ) = match_nodes!(input.into_children();
            [posting_amount(amount)] => (amount, None),
            [posting_amount(amount), posting_meta(meta)] => (amount, Some(meta)),
        );
        Ok(ret)
    }

    fn posting_cost(input: Node) -> Result<Amount> {
        let ret: Amount = match_nodes!(input.into_children();
            [number(amount), commodity_name(c)] => Amount::new(amount, c),
        );
        Ok(ret)
    }
    fn posting_total_price(input: Node) -> Result<Amount> {
        let ret: Amount = match_nodes!(input.into_children();
            [number(amount), commodity_name(c)] => Amount::new(amount, c),
        );
        Ok(ret)
    }
    fn posting_single_price(input: Node) -> Result<Amount> {
        let ret: Amount = match_nodes!(input.into_children();
            [number(amount), commodity_name(c)] => Amount::new(amount, c),
        );
        Ok(ret)
    }

    fn posting_amount(input: Node) -> Result<Amount> {
        let ret: Amount = match_nodes!(input.into_children();
            [number(amount), commodity_name(c)] => Amount::new(amount, c),
        );
        Ok(ret)
    }

    fn transaction_flag(input: Node) -> Result<Option<Flag>> {
        Ok(Some(Flag::from_str(input.as_str().trim()).unwrap()))
    }

    fn posting_price(input: Node) -> Result<SingleTotalPrice> {
        let ret: SingleTotalPrice = match_nodes!(input.into_children();
            [posting_total_price(p)] => SingleTotalPrice::Total(p),
            [posting_single_price(p)] => SingleTotalPrice::Single(p),
        );
        Ok(ret)
    }
    fn posting_meta(
        input: Node,
    ) -> Result<(Option<Amount>, Option<Date>, Option<SingleTotalPrice>)> {
        let ret: (Option<Amount>, Option<Date>, Option<SingleTotalPrice>) = match_nodes!(input.into_children();
            [] => (None, None, None),
            [posting_cost(cost)] => (Some(cost), None, None),
            [posting_price(p)] => (None, None, Some(p)),
            [posting_cost(cost), date(d)] => (Some(cost), Some(d), None),
            [posting_cost(cost), posting_price(p)] => (Some(cost), None, Some(p)),
            [posting_cost(cost), date(d), posting_price(p)] => (Some(cost), Some(d), Some(p)),
        );
        Ok(ret)
    }
    fn transaction_posting(input: Node) -> Result<Posting> {
        let ret: (
            Option<Flag>,
            Account,
            Option<(
                Amount,
                Option<(Option<Amount>, Option<Date>, Option<SingleTotalPrice>)>,
            )>,
        ) = match_nodes!(input.into_children();
            [account_name(account_name)] => (None, account_name, None),
            [account_name(account_name), posting_unit(unit)] => (None, account_name, Some(unit)),
            [transaction_flag(flag), account_name(account_name)] => (flag, account_name, None),
            [transaction_flag(flag), account_name(account_name), posting_unit(unit)] => (flag, account_name, Some(unit)),
        );

        let (flag, account, unit) = ret;

        let mut line = Posting {
            flag,
            account,
            units: None,
            cost: None,
            price: None,
            meta: Default::default(),
        };

        if let Some((amount, meta)) = unit {
            line.units = Some(amount);

            if let Some(meta) = meta {
                line.cost = meta.0;
                // line.price = meta.2; // todo
            }
        }
        Ok(line)
    }

    fn transaction_line(input: Node) -> Result<(Option<Posting>, Option<(String, AvaroString)>)> {
        let ret: (Option<Posting>, Option<(String, AvaroString)>) = match_nodes!(input.into_children();
            [transaction_posting(posting)] => (Some(posting), None),
            [commodity_line(meta)] => (None, Some(meta)),

        );
        Ok(ret)
    }
    fn transaction_lines(
        input: Node,
    ) -> Result<Vec<(Option<Posting>, Option<(String, AvaroString)>)>> {
        let ret = match_nodes!(input.into_children();
            [transaction_line(lines)..] => lines.collect(),
        );
        Ok(ret)
    }

    fn tag(input: Node) -> Result<String> {
        let ret = match_nodes!(input.into_children();
            [unquote_string(tag)] => tag.to_plain_string(),
        );
        Ok(ret)
    }
    fn link(input: Node) -> Result<String> {
        let ret = match_nodes!(input.into_children();
            [unquote_string(tag)] => tag.to_plain_string(),
        );
        Ok(ret)
    }
    fn tags(input: Node) -> Result<Vec<String>> {
        let ret = match_nodes!(input.into_children();
            [tag(tags)..] => tags.collect(),
        );
        Ok(ret)
    }
    fn links(input: Node) -> Result<Vec<String>> {
        let ret = match_nodes!(input.into_children();
            [link(links)..] => links.collect(),
        );
        Ok(ret)
    }

    fn transaction(input: Node) -> Result<Directive> {
        let ret: (
            Date,
            Option<Flag>,
            Option<AvaroString>,
            Option<AvaroString>,
            Vec<String>,
            Vec<String>,
            Vec<(Option<Posting>, Option<(String, AvaroString)>)>,
        ) = match_nodes!(input.into_children();
            [date(date), transaction_flag(flag), tags(tags), links(links), transaction_lines(lines)] => (date, flag, None, None, tags, links, lines),
            [date(date), transaction_flag(flag), quote_string(narration), tags(tags), links(links), transaction_lines(lines)] => (date, flag, None, Some(narration), tags, links, lines),
            [date(date), transaction_flag(flag), quote_string(payee), quote_string(narration), tags(tags), links(links), transaction_lines(lines)] => (date, flag, Some(payee), Some(narration), tags, links,lines),
        );
        let mut transaction = Transaction {
            date: ret.0,
            flag: ret.1,
            payee: ret.2,
            narration: ret.3,
            tags: ret.4.into_iter().collect(),
            links: ret.5.into_iter().collect(),
            postings: vec![],
            meta: HashMap::default(),
        };

        for line in ret.6 {
            match line {
                (Some(trx), None) => {
                    transaction.postings.push(trx);
                }
                (None, Some(meta)) => {
                    transaction.meta.insert(meta.0, meta.1);
                }
                _ => {}
            }
        }

        Ok(Directive::Transaction(transaction))
    }

    fn commodity(input: Node) -> Result<Directive> {
        let ret = match_nodes!(input.into_children();
            [date(date), commodity_name(name)] => (date, name, vec![]),
            [date(date), commodity_name(name), commodity_meta(meta)] => (date, name, meta),
        );
        Ok(Directive::Commodity(Commodity {
            date: ret.0,
            currency: ret.1,
            meta: ret.2.into_iter().collect(),
        }))
    }

    fn string_or_account(input: Node) -> Result<StringOrAccount> {
        let ret: StringOrAccount = match_nodes!(input.into_children();
            [string(value)] => StringOrAccount::String(value),
            [account_name(value)] => StringOrAccount::Account(value),
        );
        Ok(ret)
    }

    fn custom(input: Node) -> Result<Directive> {
        let ret: (Date, AvaroString, Vec<StringOrAccount>) = match_nodes!(input.into_children();
            [date(date), string(module), string_or_account(options)..] => (date, module, options.collect()),
        );
        Ok(Directive::Custom(Custom {
            date: ret.0,
            custom_type: ret.1,
            values: ret.2,
            meta: Default::default(),
        }))
    }

    fn include(input: Node) -> Result<Directive> {
        let ret: AvaroString = match_nodes!(input.into_children();
            [quote_string(path)] => path,
        );
        Ok(Directive::Include { file: ret })
    }

    fn note(input: Node) -> Result<Directive> {
        let ret: (Date, Account, AvaroString) = match_nodes!(input.into_children();
            [date(date), account_name(a), string(path)] => (date, a, path),
        );
        Ok(Directive::Note(Note {
            date: ret.0,
            account: ret.1,
            comment: ret.2,
            tags: None,
            links: None,
            meta: Default::default(),
        }))
    }

    fn pad(input: Node) -> Result<Directive> {
        let ret: (Date, Account, Account) = match_nodes!(input.into_children();
            [date(date), account_name(from), account_name(to)] => (date, from, to),
        );
        Ok(Directive::Pad(Pad {
            date: ret.0,
            account: ret.1,
            source: ret.2,
            meta: Default::default(),
        }))
    }

    fn event(input: Node) -> Result<Directive> {
        let ret: (Date, AvaroString, AvaroString) = match_nodes!(input.into_children();
            [date(date), string(name), string(value)] => (date, name, value),
        );
        Ok(Directive::Event(Event {
            date: ret.0,
            event_type: ret.1,
            description: ret.2,
            meta: Default::default(),
        }))
    }

    fn balance(input: Node) -> Result<Directive> {
        let ret: (Date, Account, BigDecimal, String) = match_nodes!(input.into_children();
            [date(date), account_name(name), number(amount), commodity_name(commodity)] => (date, name, amount, commodity),
        );
        Ok(Directive::Balance(Balance {
            date: ret.0,
            account: ret.1,
            amount: Amount::new(ret.2, ret.3),
            tolerance: None,
            diff_amount: None,
            meta: Default::default(),
        }))
    }

    fn document(input: Node) -> Result<Directive> {
        let ret: (Date, Account, AvaroString) = match_nodes!(input.into_children();
            [date(date), account_name(name), string(path)] => (date, name, path),
        );
        Ok(Directive::Document(Document {
            date: ret.0,
            account: ret.1,
            filename: ret.2,
            tags: None,
            links: None,
            meta: Default::default(),
        }))
    }

    fn price(input: Node) -> Result<Directive> {
        let ret: (Date, String, BigDecimal, String) = match_nodes!(input.into_children();
            [date(date), commodity_name(source), number(price), commodity_name(target)] => (date, source, price, target)
        );
        Ok(Directive::Price(Price {
            date: ret.0,
            currency: ret.1,
            amount: Amount::new(ret.2, ret.3),
            meta: Default::default(),
        }))
    }

    fn item(input: Node) -> Result<Directive> {
        let ret = match_nodes!(input.into_children();
            [option(item)] => item,
            [open(item)] => item,
            [plugin(item)] => item,
            [close(item)] => item,
            [include(item)] => item,
            [note(item)] => item,
            [pad(item)] => item,
            [event(item)] => item,
            [document(item)] => item,
            [balance(item)] => item,
            [price(item)] => item,
            [commodity(item)] => item,
            [custom(item)] => item,
            [comment(item)] => item,
            [transaction(item)] => item,
        );
        Ok(ret)
    }
    fn entry(input: Node) -> Result<Vec<Directive>> {
        let ret = match_nodes!(input.into_children();
            [item(items).., _] => items.collect(),
        );
        Ok(ret)
    }
}

pub fn parse_avaro(input_str: &str) -> Result<Vec<Directive>> {
    let inputs = AvaroParser::parse(Rule::entry, input_str)?;
    let input = inputs.single()?;
    AvaroParser::entry(input)
}

pub fn parse_account(input_str: &str) -> Result<Account> {
    let inputs = AvaroParser::parse(Rule::account_name, input_str)?;
    let input = inputs.single()?;
    AvaroParser::account_name(input)
}
