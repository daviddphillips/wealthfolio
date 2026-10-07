//! Adjusted book costs survive transfers, fees and WAC returns of capital.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
mod support;
use std::str::FromStr;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use support::*;
use wealthfolio_portfolio_engine::model::{AccountId, AssetId};

fn inline(yaml: &str) -> Pipeline {
    Pipeline::from_scenario(&serde_yaml::from_str::<Scenario>(yaml).unwrap())
}

#[test]
fn wac_roc_with_transferred_lot_uses_whole_position_basis() {
    let p = inline(
        r#"
id: BASIS-WAC-TRANSFERRED-ROC
policy: { base_currency: CAD, timezone: UTC, as_of: 2025-01-06 }
accounts:
  - { id: a, currency: CAD, cost_basis_method: WAC }
assets:
  - { id: ETF, quote_ccy: CAD }
activities:
  - { id: tin, account: a, type: TRANSFER_IN, date: 2025-01-02T10:00:00Z, asset: ETF, quantity: 1, unit_price: 1, metadata: { flow: { is_external: true } } }
  - { id: buy, account: a, type: BUY, date: 2025-01-03T10:00:00Z, asset: ETF, quantity: 1, unit_price: 10, amount: 10 }
  - { id: roc, account: a, type: DIVIDEND, subtype: RETURN_OF_CAPITAL, date: 2025-01-06T10:00:00Z, asset: ETF, amount: 4 }
quotes:
  - { asset: ETF, day: 2025-01-02, close: 10 }
  - { asset: ETF, day: 2025-01-06, close: 10 }
"#,
    );
    let remaining: rust_decimal::Decimal = p.lots().iter().map(|l| l.remaining_cost_basis).sum();
    assert_eq!(remaining, dec!(7));
    assert!(p.bundle.disposals.is_empty());
}

const TRANSFER: &str = r#"
id: BASIS-ADJUSTED-TRANSFER
policy: { base_currency: CAD, timezone: UTC, as_of: 2025-01-08 }
accounts:
  - { id: a, currency: CAD }
  - { id: b, currency: CAD }
assets:
  - { id: ETF, quote_ccy: USD }
activities:
  - { id: depa, account: a, type: DEPOSIT, date: 2025-01-02T09:00:00Z, amount: 500 }
  - { id: depb, account: b, type: DEPOSIT, date: 2025-01-02T09:01:00Z, amount: 500 }
  - { id: buy, account: a, type: BUY, date: 2025-01-03T10:00:00Z, asset: ETF, quantity: 10, unit_price: 10, amount: 100, currency: USD, fx_rate: 1.2 }
  - { id: nd, account: a, type: ADJUSTMENT, subtype: NOTIONAL_DISTRIBUTION, date: 2025-01-06T10:00:00Z, asset: ETF, amount: 20, currency: USD }
  - { id: out, account: a, type: TRANSFER_OUT, date: 2025-01-08T10:00:00Z, asset: ETF, quantity: 10, unit_price: 12, source_group_id: g }
  - { id: in, account: b, type: TRANSFER_IN, date: 2025-01-08T11:00:00Z, asset: ETF, quantity: 10, unit_price: 12, source_group_id: g, fee: TRANSFER_FEE, currency: USD }
quotes:
  - { asset: ETF, day: 2025-01-03, close: 10 }
  - { asset: ETF, day: 2025-01-08, close: 10 }
fx_rates:
  - { from: USD, to: CAD, day: 2025-01-03, rate: 1.2 }
  - { from: USD, to: CAD, day: 2025-01-06, rate: 1.4 }
  - { from: USD, to: CAD, day: 2025-01-08, rate: 1.4 }
"#;

#[test]
fn transfer_cover_slices_adjusted_book_cost() {
    let yaml = TRANSFER.replace("TRANSFER_FEE", "0").replace(
        "  - { id: nd,", 
        "  - { id: short, account: b, type: SELL, subtype: POSITION_OPEN, date: 2025-01-06T09:00:00Z, asset: ETF, quantity: 4, unit_price: 10, amount: 40, currency: USD, fx_rate: 1.4 }\n  - { id: nd,"
    );
    let p = inline(&yaml);
    let b = &p.bundle.final_state.accounts[&AccountId::new("b")].positions[&AssetId::new("ETF")];
    assert_eq!(b.cost_basis_base.unwrap().round_dp(8), dec!(88.8));
    let d = p
        .bundle
        .disposals
        .iter()
        .find(|d| d.event.as_str() == "in")
        .unwrap();
    assert_eq!(d.realized_pnl_base, dec!(-3.2));
}

#[test]
fn transfer_fee_is_added_to_adjusted_book_cost() {
    let p = inline(&TRANSFER.replace("TRANSFER_FEE", "5"));
    let b = &p.bundle.final_state.accounts[&AccountId::new("b")].positions[&AssetId::new("ETF")];
    assert_eq!(b.total_cost_basis, dec!(125));
    // Existing transfers capitalize charges at their carried acquisition rate.
    assert_eq!(b.cost_basis_base.unwrap().round_dp(8), dec!(154));
}

#[test]
fn transfer_cover_without_adjustment_is_a_passing_control() {
    let yaml = TRANSFER.replace("TRANSFER_FEE", "0").replace(
        "  - { id: nd,", 
        "  - { id: short, account: b, type: SELL, subtype: POSITION_OPEN, date: 2025-01-06T09:00:00Z, asset: ETF, quantity: 4, unit_price: 10, amount: 40, currency: USD, fx_rate: 1.4 }\n  - { id: nd,"
    );
    let yaml = yaml
        .lines()
        .filter(|line| !line.contains("{ id: nd,"))
        .collect::<Vec<_>>()
        .join("\n");
    let p = inline(&yaml);
    let b = &p.bundle.final_state.accounts[&AccountId::new("b")].positions[&AssetId::new("ETF")];
    assert_eq!(b.cost_basis_base.unwrap().round_dp(8), dec!(72));
    let d = p
        .bundle
        .disposals
        .iter()
        .find(|d| d.event.as_str() == "in")
        .unwrap();
    assert_eq!(d.realized_pnl_base, dec!(8));
}

#[test]
fn transfer_fee_without_adjustment_is_a_passing_control() {
    let yaml = TRANSFER.replace("TRANSFER_FEE", "5");
    let yaml = yaml
        .lines()
        .filter(|line| !line.contains("{ id: nd,"))
        .collect::<Vec<_>>()
        .join("\n");
    let p = inline(&yaml);
    let b = &p.bundle.final_state.accounts[&AccountId::new("b")].positions[&AssetId::new("ETF")];
    assert_eq!(b.total_cost_basis, dec!(105));
    assert_eq!(b.cost_basis_base.unwrap().round_dp(8), dec!(126));
}

const WAC_FOREIGN: &str = r#"
id: BASIS-WAC-FOREIGN
policy: { base_currency: CAD, timezone: UTC, as_of: 2025-01-08 }
accounts:
  - { id: a, currency: CAD, cost_basis_method: WAC }
assets:
  - { id: ETF, quote_ccy: USD }
activities:
  - { id: dep, account: a, type: DEPOSIT, date: 2025-01-01T09:00:00Z, amount: 100 }
  - { id: tin, account: a, type: TRANSFER_IN, date: 2025-01-02T10:00:00Z, asset: ETF, quantity: 1, unit_price: 1, currency: USD, metadata: { flow: { is_external: true } } }
  - { id: buy, account: a, type: BUY, date: 2025-01-03T10:00:00Z, asset: ETF, quantity: 1, unit_price: 10, amount: 10, currency: USD, fx_rate: 3 }
  - { id: roc, account: a, type: DIVIDEND, subtype: RETURN_OF_CAPITAL, date: 2025-01-06T10:00:00Z, asset: ETF, amount: 4, currency: USD }
quotes:
  - { asset: ETF, day: 2025-01-02, close: 10 }
  - { asset: ETF, day: 2025-01-08, close: 10 }
fx_rates:
  - { from: USD, to: CAD, day: 2025-01-02, rate: 1 }
  - { from: USD, to: CAD, day: 2025-01-03, rate: 3 }
  - { from: USD, to: CAD, day: 2025-01-06, rate: 4 }
"#;

#[test]
fn wac_roc_uses_each_currency_pool_and_keeps_transfer_provenance() {
    let p = inline(WAC_FOREIGN);
    let a = &p.bundle.final_state.accounts[&AccountId::new("a")];
    let position = &a.positions[&AssetId::new("ETF")];
    assert_eq!(position.total_cost_basis.round_dp(8), dec!(7));
    assert_eq!(position.cost_basis_account.unwrap().round_dp(8), dec!(15));
    assert_eq!(position.cost_basis_base.unwrap().round_dp(8), dec!(15));
    assert!(p.bundle.disposals.is_empty());
    assert_eq!(position.lots.len(), 2);
    assert!(position.lots.iter().any(|lot| lot
        .source_event
        .as_ref()
        .is_some_and(|id| id.as_str() == "tin")));
}

#[test]
fn wac_sale_after_roc_relieves_the_average_adjusted_basis() {
    let yaml = WAC_FOREIGN.replace("quotes:", "  - { id: sell, account: a, type: SELL, date: 2025-01-08T10:00:00Z, asset: ETF, quantity: 1, unit_price: 10, amount: 10, currency: USD }\nquotes:");
    let p = inline(&yaml);
    let position =
        &p.bundle.final_state.accounts[&AccountId::new("a")].positions[&AssetId::new("ETF")];
    assert_eq!(position.total_cost_basis.round_dp(8), dec!(3.5));
    assert_eq!(position.cost_basis_base.unwrap().round_dp(8), dec!(7.5));
    let basis: Decimal = p.bundle.disposals.iter().map(|d| d.cost_basis_base).sum();
    assert_eq!(basis, dec!(7.5));
}

#[test]
fn repeated_wac_roc_realizes_only_the_excess_of_each_currency_pool() {
    let yaml = WAC_FOREIGN.replace("quotes:", "  - { id: roc2, account: a, type: DIVIDEND, subtype: RETURN_OF_CAPITAL, date: 2025-01-08T10:00:00Z, asset: ETF, amount: 8, currency: USD }\nquotes:");
    let p = inline(&yaml);
    let position =
        &p.bundle.final_state.accounts[&AccountId::new("a")].positions[&AssetId::new("ETF")];
    assert_eq!(position.total_cost_basis, dec!(0));
    assert_eq!(position.cost_basis_base, Some(dec!(0)));
    let local: Decimal = p.bundle.disposals.iter().map(|d| d.realized_pnl).sum();
    let base: Decimal = p.bundle.disposals.iter().map(|d| d.realized_pnl_base).sum();
    assert_eq!(local, dec!(1));
    assert_eq!(base, dec!(17));
    assert!(p.bundle.disposals.iter().all(|d| d.quantity.is_zero()));
}

#[test]
fn wac_roc_keeps_foreign_book_cost_after_local_basis_reaches_zero() {
    let yaml = WAC_FOREIGN
        .replace("amount: 4,", "amount: 11,")
        .replace("rate: 4 }", "rate: 0.5 }");
    let p = inline(&yaml);
    let position =
        &p.bundle.final_state.accounts[&AccountId::new("a")].positions[&AssetId::new("ETF")];
    assert_eq!(position.total_cost_basis, dec!(0));
    assert_eq!(position.cost_basis_base.unwrap().round_dp(8), dec!(25.5));
    let lots: Decimal = p
        .lots()
        .iter()
        .map(|lot| lot.remaining_cost_basis_base)
        .sum();
    assert_eq!(lots.round_dp(8), dec!(25.5));
}

fn performance(yaml: &str) -> serde_json::Value {
    let scenario: Scenario = serde_yaml::from_str(yaml).unwrap();
    let p = Pipeline::from_scenario(&scenario);
    let body = capture_body(&p, &all_windows(&scenario));
    body["portfolio"]["all_time"].clone()
}

fn number(value: &serde_json::Value) -> Decimal {
    Decimal::from_str(value.as_str().expect("decimal string")).unwrap()
}

#[test]
fn adjusted_transfers_preserve_total_return_and_reconcile_attribution() {
    let yaml = TRANSFER.replace("TRANSFER_FEE", "5");
    let without_adjustment = yaml
        .lines()
        .filter(|line| !line.contains("{ id: nd,"))
        .collect::<Vec<_>>()
        .join("\n");
    let adjusted = performance(&yaml);
    let original = performance(&without_adjustment);
    assert_eq!(adjusted["summary"]["amount"], original["summary"]["amount"]);
    assert_eq!(adjusted["returns"], original["returns"]);
    let attribution = &adjusted["attribution"];
    let explained = number(&attribution["income"])
        + number(&attribution["realized_pnl"])
        + number(&attribution["unrealized_pnl_change"])
        + number(&attribution["fx_effect"])
        - number(&attribution["fees"])
        - number(&attribution["taxes"]);
    assert_eq!(
        explained.round_dp(8),
        number(&adjusted["summary"]["amount"])
    );
    assert_eq!(number(&attribution["residual"]), dec!(0));
}

#[test]
fn later_roc_reclassification_preserves_cash_and_total_performance() {
    let reclassified = WAC_FOREIGN.replace("subtype: RETURN_OF_CAPITAL, ", "").replace("quotes:", "  - { id: reclassify, account: a, type: ADJUSTMENT, subtype: RETURN_OF_CAPITAL, date: 2025-01-08T10:00:00Z, asset: ETF, amount: 4, currency: USD }\nquotes:");
    let direct = inline(WAC_FOREIGN);
    let later = inline(&reclassified);
    let account = AccountId::new("a");
    assert_eq!(
        direct.bundle.final_state.accounts[&account].cash,
        later.bundle.final_state.accounts[&account].cash
    );
    let original = performance(WAC_FOREIGN);
    let corrected = performance(&reclassified);
    assert_eq!(original["summary"], corrected["summary"]);
    assert_eq!(original["returns"], corrected["returns"]);
    assert_eq!(original["attribution"], corrected["attribution"]);
}

#[test]
fn transfer_fee_updates_distinct_account_and_base_book_costs() {
    let yaml = TRANSFER.replace("TRANSFER_FEE", "5").replace("base_currency: CAD", "base_currency: EUR")
        + "  - { from: USD, to: EUR, day: 2025-01-03, rate: 1.5 }\n  - { from: USD, to: EUR, day: 2025-01-06, rate: 1.8 }\n  - { from: USD, to: EUR, day: 2025-01-08, rate: 1.8 }\n";
    let p = inline(&yaml);
    let position =
        &p.bundle.final_state.accounts[&AccountId::new("b")].positions[&AssetId::new("ETF")];
    // CAD: 120 purchase + 28 reinvestment + 6 fee. EUR: 150 + 36 + 7.5.
    assert_eq!(position.cost_basis_account.unwrap().round_dp(8), dec!(154));
    assert_eq!(position.cost_basis_base.unwrap().round_dp(8), dec!(193.5));
}
