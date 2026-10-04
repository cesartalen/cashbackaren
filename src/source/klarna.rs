use super::{Http, Source, keep, merchant, script_json};
use crate::offer::{Kind, Offer};
use anyhow::{Context, Result, ensure};
use serde_json::Value;

const URL: &str = "https://www.klarna.com/se/store/?type=CASHBACK&page=";

pub const SOURCE: Source = Source {
    name: "klarna",
    url: "https://www.klarna.com/se/store/",
    min: 100,
    fetch,
};

fn fetch(http: &Http) -> Result<Vec<Offer>> {
    let mut offers = Vec::new();
    let mut seen = 0;
    for page in 1.. {
        let payload = script_json(&http.get(&format!("{URL}{page}"))?, "initial_payload")?;
        let (stores, total) = listing(&payload)?;
        ensure!(!stores.is_empty(), "page {page} is empty");
        seen += stores.len();
        offers.extend(stores.iter().filter_map(|s| keep(SOURCE.name, offer(s))));
        if seen >= total {
            break;
        }
    }
    Ok(offers)
}

// Returns the page's stores and the site's total.
fn listing(payload: &Value) -> Result<(&Vec<Value>, usize)> {
    let listing = payload["__DEHYDRATED_QUERY_STATE__"]["queries"]
        .as_array()
        .and_then(|qs| qs.iter().find(|q| q["queryKey"][0] == "STORE_DIRECTORY_LISTING"))
        .map(|q| &q["state"]["data"]["pages"][0])
        .context("no store listing")?;
    let total = listing["totalHits"].as_u64().context("no totalHits")? as usize;
    let stores = listing["stores"].as_array().context("no stores")?;
    Ok((stores, total))
}

fn offer(s: &Value) -> Result<Offer> {
    let name = s["displayName"].as_str().context("no displayName")?;
    let cashback = &s["cashbackDiscount"];
    let site = s["otcUrl"].as_str().and_then(|u| u.split_once("merchantUrl="));
    Ok(Offer {
        merchant: merchant(site.map(|(_, u)| u), name),
        name: name.into(),
        kind: Kind::Percent,
        amount: cashback["discountPercentage"]
            .as_i64()
            .with_context(|| format!("no rate for {name}"))?,
        up_to: cashback["showUpToPrefix"].as_bool().unwrap_or(false),
        category: s["category"].as_str().map(Into::into),
        url: match s["storeUrl"].as_str() {
            Some(path) => format!("https://www.klarna.com{path}"),
            None => SOURCE.url.into(),
        },
    })
}
