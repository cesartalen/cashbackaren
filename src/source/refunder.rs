use super::{Http, Source, keep, merchant};
use crate::offer::{Kind, Offer};
use anyhow::{Context, Result, ensure};
use serde_json::Value;

const URL: &str = "https://www.refunder.se/api/public/stores?limit=50&offset=";

pub const SOURCE: Source = Source {
    name: "refunder",
    url: "https://www.refunder.se/",
    min: 500,
    fetch,
};

fn fetch(http: &Http) -> Result<Vec<Offer>> {
    let mut offers = Vec::new();
    for offset in (0..).step_by(50) {
        let page: Value = serde_json::from_str(&http.get(&format!("{URL}{offset}"))?)?;
        let total = page["total"].as_u64().context("no total")? as usize;
        let stores = page["stores"].as_array().context("no stores")?;
        ensure!(!stores.is_empty(), "offset {offset} is empty");
        offers.extend(stores.iter().filter_map(|s| keep(SOURCE.name, offer(s))));
        if offset + stores.len() >= total {
            break;
        }
    }
    Ok(offers)
}

fn offer(s: &Value) -> Result<Offer> {
    let name = s["name"].as_str().context("no name")?;
    let cashback = s["cashback"].as_str().unwrap_or_default();
    let (kind, amount, up_to) =
        rate(cashback).with_context(|| format!("bad cashback {cashback:?} for {name}"))?;
    Ok(Offer {
        merchant: merchant(s["domain"].as_str(), name),
        name: name.into(),
        kind,
        amount,
        up_to,
        category: s["category"].as_str().map(Into::into),
        url: s["store_page_url"]
            .as_str()
            .with_context(|| format!("no url for {name}"))?
            .into(),
    })
}

// Parses "Upp till 12,5%" or "50 kr".
fn rate(text: &str) -> Option<(Kind, i64, bool)> {
    let (up_to, text) = match text.strip_prefix("Upp till ") {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (kind, number) = match text.strip_suffix('%') {
        Some(number) => (Kind::Percent, number),
        None => (Kind::Fixed, text.strip_suffix(" kr")?),
    };
    let amount = number.replace(',', ".").parse::<f64>().ok()? * 100.0;
    Some((kind, amount.round() as i64, up_to))
}
