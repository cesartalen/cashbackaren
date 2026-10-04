pub mod offer;
pub mod source;
pub mod store;

use anyhow::{Result, anyhow, ensure};
use offer::Offer;
use source::{Http, Source};
use std::thread;
use store::Store;

pub fn run(store: &mut Store) -> Result<()> {
    let http = Http::default();
    let results: Vec<_> = thread::scope(|s| {
        let handles: Vec<_> = source::ALL
            .iter()
            .map(|src| s.spawn(|| fetch(src, &http)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Err(anyhow!("panicked"))))
            .collect()
    });
    let mut failed = 0;
    for (src, result) in source::ALL.iter().zip(results) {
        match result {
            Ok(offers) => {
                store.replace(src, &offers)?;
                eprintln!("{}: {} offers", src.name, offers.len());
            }
            Err(e) => {
                failed += 1;
                store.fail(src, &format!("{e:#}"))?;
                eprintln!("{}: {e:#}", src.name);
            }
        }
    }
    ensure!(failed == 0, "{failed} sources failed");
    Ok(())
}

fn fetch(src: &Source, http: &Http) -> Result<Vec<Offer>> {
    let mut offers = (src.fetch)(http)?;
    offers.sort_by(|a, b| a.merchant.cmp(&b.merchant));
    offers.dedup();
    ensure!(
        offers.len() >= src.min,
        "only {} offers, expected at least {}",
        offers.len(),
        src.min
    );
    Ok(offers)
}
