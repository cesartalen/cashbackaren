use super::{Http, Source, keep};
use crate::offer::{Kind, Offer};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::HashMap;

const API: &str = "https://onlineshopping.loyaltykey.com/api/v1/shops";
const SHOPS: &str = "?filter%5Bchannel%5D=SAS&filter%5Blanguage%5D=sv&filter%5Bcountry%5D=SE&filter%5Bamount%5D=5000";
const CATEGORIES: &str = "/categories?filter%5Blanguage%5D=sv";

pub const SOURCE: Source = Source {
    name: "sas",
    url: "https://onlineshopping.flysas.com/sv-SE/",
    min: 300,
    fetch,
};

fn fetch(http: &Http) -> Result<Vec<Offer>> {
    let categories = data(http, CATEGORIES)?;
    let categories = categories
        .iter()
        .filter_map(|c| Some((c["category_id"].as_i64()?, c["name"].as_str()?)))
        .collect();
    Ok(data(http, SHOPS)?
        .iter()
        .filter_map(|s| keep(SOURCE.name, offer(s, &categories)))
        .collect())
}

// Returns the endpoint's data array.
fn data(http: &Http, path: &str) -> Result<Vec<Value>> {
    let body: Value = serde_json::from_str(&http.get(&format!("{API}{path}"))?)?;
    body["data"].as_array().cloned().context("no data")
}

fn offer(s: &Value, categories: &HashMap<i64, &str>) -> Result<Offer> {
    let name = s["name"].as_str().context("no name")?;
    let kind = match s["commission_type"].as_str() {
        Some("variable") => Kind::Points,
        Some("fixed") => Kind::FixedPoints,
        other => bail!("bad commission type {other:?} for {name}"),
    };
    let points = |key| s[key].as_i64().filter(|&p| p > 0);
    Ok(Offer {
        merchant: name.to_lowercase(),
        name: name.into(),
        kind,
        // A campaign replaces the regular points.
        amount: points("points_campaign")
            .or_else(|| points("points"))
            .with_context(|| format!("no points for {name}"))?,
        up_to: false,
        category: s["categoryId"]
            .as_i64()
            .and_then(|id| categories.get(&id))
            .map(|&c| c.into()),
        url: match s["slug"].as_str().zip(s["uuid"].as_str()) {
            Some((slug, uuid)) => format!("{}butiker/{slug}/{uuid}", SOURCE.url),
            None => SOURCE.url.into(),
        },
    })
}
