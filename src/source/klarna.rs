use super::{Http, Source, domain, script_json};
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
    for page in 1.. {
        let (found, total) = parse(&http.get(&format!("{URL}{page}"))?)?;
        ensure!(!found.is_empty(), "page {page} is empty");
        offers.extend(found);
        if offers.len() >= total {
            break;
        }
    }
    Ok(offers)
}

// Returns the page's offers and the site's total.
fn parse(html: &str) -> Result<(Vec<Offer>, usize)> {
    let payload = script_json(html, "initial_payload")?;
    let page = payload["__DEHYDRATED_QUERY_STATE__"]["queries"]
        .as_array()
        .and_then(|qs| qs.iter().find(|q| q["queryKey"][0] == "STORE_DIRECTORY_LISTING"))
        .map(|q| &q["state"]["data"]["pages"][0])
        .context("no store listing")?;
    let total = page["totalHits"].as_u64().context("no totalHits")? as usize;
    let stores = page["stores"].as_array().context("no stores")?;
    Ok((stores.iter().map(offer).collect::<Result<_>>()?, total))
}

fn offer(s: &Value) -> Result<Offer> {
    let name = s["displayName"].as_str().context("no displayName")?;
    let cashback = &s["cashbackDiscount"];
    let merchant = s["otcUrl"].as_str().and_then(|u| u.split_once("merchantUrl="));
    Ok(Offer {
        merchant: merchant.map_or_else(|| name.to_lowercase(), |(_, u)| domain(u)),
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
