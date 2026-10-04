mod klarna;
mod refunder;

use crate::offer::Offer;
use anyhow::{Context, Result};
use serde_json::Value;
use std::time::Duration;

pub struct Source {
    pub name: &'static str,
    pub url: &'static str,
    // Fewer offers than this means the site changed.
    pub min: usize,
    pub fetch: fn(&Http) -> Result<Vec<Offer>>,
}

// One module per site; register it here.
pub const ALL: &[Source] = &[klarna::SOURCE, refunder::SOURCE];

const USER_AGENT: &str = concat!(
    "cashbackaren/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/cesartalen/cashbackaren)"
);

pub struct Http(ureq::Agent);

impl Default for Http {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Self(config.into())
    }
}

impl Http {
    pub fn get(&self, url: &str) -> Result<String> {
        Ok(self.0.get(url).call()?.body_mut().read_to_string()?)
    }
}

pub fn script_json(html: &str, id: &str) -> Result<Value> {
    let json = html
        .split_once(&format!(r#"<script id="{id}""#))
        .and_then(|(_, s)| s.split_once('>'))
        .and_then(|(_, s)| s.split_once("</script>"))
        .with_context(|| format!("no script {id}"))?
        .0;
    Ok(serde_json::from_str(json)?)
}

// Returns the bare host of a URL, or None if it isn't a domain.
pub fn domain(url: &str) -> Option<String> {
    let url = url.trim().to_lowercase();
    let host = url.split_once("://").map_or(url.as_str(), |(_, s)| s);
    let host = host.split(['/', '?', '&']).next()?.trim_start_matches("www.");
    (host.contains('.') && !host.contains(' ')).then(|| host.into())
}
