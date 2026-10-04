use crate::offer::Offer;
use crate::source::Source;
use anyhow::Result;
use rusqlite::{Connection, params};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS source (
    name       TEXT PRIMARY KEY,
    url        TEXT NOT NULL,
    scraped_at TEXT,
    offers     INTEGER,
    error      TEXT
);
CREATE TABLE IF NOT EXISTS offer (
    source   TEXT NOT NULL REFERENCES source,
    merchant TEXT NOT NULL,
    name     TEXT NOT NULL,
    kind     TEXT NOT NULL,
    amount   INTEGER NOT NULL,
    up_to    INTEGER NOT NULL,
    category TEXT,
    url      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS offer_merchant ON offer (merchant);
";

pub struct Store(Connection);

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self(conn))
    }

    // Replaces the source's offers.
    pub fn replace(&mut self, src: &Source, offers: &[Offer]) -> Result<()> {
        let tx = self.0.transaction()?;
        tx.execute(
            "INSERT INTO source (name, url, scraped_at, offers) VALUES (?1, ?2, CURRENT_TIMESTAMP, ?3)
             ON CONFLICT (name) DO UPDATE SET url = ?2, scraped_at = CURRENT_TIMESTAMP, offers = ?3, error = NULL",
            params![src.name, src.url, offers.len() as i64],
        )?;
        tx.execute("DELETE FROM offer WHERE source = ?1", [src.name])?;
        for o in offers {
            tx.execute(
                "INSERT INTO offer VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![src.name, o.merchant, o.name, o.kind.as_str(), o.amount, o.up_to, o.category, o.url],
            )?;
        }
        Ok(tx.commit()?)
    }

    // Records the error and keeps the source's previous offers.
    pub fn fail(&self, src: &Source, error: &str) -> Result<()> {
        self.0.execute(
            "INSERT INTO source (name, url, error) VALUES (?1, ?2, ?3)
             ON CONFLICT (name) DO UPDATE SET url = ?2, error = ?3",
            params![src.name, src.url, error],
        )?;
        Ok(())
    }
}
