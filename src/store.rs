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
CREATE TABLE IF NOT EXISTS offer_history (
    id           INTEGER PRIMARY KEY,
    source       TEXT NOT NULL REFERENCES source,
    merchant     TEXT NOT NULL,
    name         TEXT NOT NULL,
    kind         TEXT NOT NULL,
    amount       INTEGER NOT NULL,
    up_to        INTEGER NOT NULL,
    category     TEXT,
    url          TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    last_seen_at  TEXT NOT NULL,
    ended_at      TEXT
);
CREATE INDEX IF NOT EXISTS offer_history_merchant ON offer_history (merchant, source, id);
CREATE INDEX IF NOT EXISTS offer_history_active ON offer_history (source, merchant) WHERE ended_at IS NULL;
";

pub struct Store(Connection);

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        Self::initialize(Connection::open(path)?)
    }

    fn initialize(mut conn: Connection) -> Result<Self> {
        let tx = conn.transaction()?;
        let has_history: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'offer_history')",
            [],
            |row| row.get(0),
        )?;
        tx.execute_batch(SCHEMA)?;
        if !has_history {
            tx.execute_batch(
                "INSERT INTO offer_history
                 (source, merchant, name, kind, amount, up_to, category, url, first_seen_at, last_seen_at)
                 SELECT DISTINCT o.source, o.merchant, o.name, o.kind, o.amount, o.up_to, o.category, o.url,
                        COALESCE(s.scraped_at, CURRENT_TIMESTAMP), COALESCE(s.scraped_at, CURRENT_TIMESTAMP)
                 FROM offer o LEFT JOIN source s ON s.name = o.source",
            )?;
        }
        tx.commit()?;
        Ok(Self(conn))
    }

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
                params![
                    src.name,
                    o.merchant,
                    o.name,
                    o.kind.as_str(),
                    o.amount,
                    o.up_to,
                    o.category,
                    o.url
                ],
            )?;
        }
        tx.execute(
            "UPDATE offer_history AS h SET ended_at = (SELECT scraped_at FROM source WHERE name = ?1)
             WHERE h.source = ?1 AND h.ended_at IS NULL AND NOT EXISTS (
                 SELECT 1 FROM offer o WHERE
                 (o.source, o.merchant, o.name, o.kind, o.amount, o.up_to, o.category, o.url)
                 IS (h.source, h.merchant, h.name, h.kind, h.amount, h.up_to, h.category, h.url)
             )",
            [src.name],
        )?;
        tx.execute(
            "UPDATE offer_history SET last_seen_at = (SELECT scraped_at FROM source WHERE name = ?1)
             WHERE source = ?1 AND ended_at IS NULL",
            [src.name],
        )?;
        tx.execute(
            "INSERT INTO offer_history
             (source, merchant, name, kind, amount, up_to, category, url, first_seen_at, last_seen_at)
             SELECT DISTINCT o.source, o.merchant, o.name, o.kind, o.amount, o.up_to, o.category, o.url,
                    s.scraped_at, s.scraped_at
             FROM offer o JOIN source s ON s.name = o.source
             WHERE o.source = ?1 AND NOT EXISTS (
                 SELECT 1 FROM offer_history h WHERE h.ended_at IS NULL AND
                 (o.source, o.merchant, o.name, o.kind, o.amount, o.up_to, o.category, o.url)
                 IS (h.source, h.merchant, h.name, h.kind, h.amount, h.up_to, h.category, h.url)
             )",
            [src.name],
        )?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offer::Kind;

    const SOURCE: Source = Source {
        name: "test",
        url: "https://example.com",
        min: 0,
        fetch: |_| Ok(Vec::new()),
    };

    fn offer(amount: i64) -> Offer {
        Offer {
            merchant: "shop".into(),
            name: "Shop".into(),
            kind: Kind::Percent,
            amount,
            up_to: false,
            category: None,
            url: "https://example.com/shop".into(),
        }
    }

    fn history(store: &Store) -> Vec<(i64, bool)> {
        store
            .0
            .prepare("SELECT amount, ended_at IS NOT NULL FROM offer_history ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    #[test]
    fn tracks_changes_removal_and_reappearance() -> Result<()> {
        let mut store = Store::open(":memory:")?;
        store.replace(&SOURCE, &[offer(500)])?;
        store.0.execute_batch(
            "UPDATE offer_history SET first_seen_at = '2026-01-01 00:00:00', last_seen_at = '2026-01-01 00:00:00'",
        )?;
        store.replace(&SOURCE, &[offer(500), offer(500)])?;
        assert_eq!(history(&store), [(500, false)]);
        let (first, last): (String, String) = store.0.query_row(
            "SELECT first_seen_at, last_seen_at FROM offer_history",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(first, "2026-01-01 00:00:00");
        assert!(last > first);

        store.replace(&SOURCE, &[offer(750)])?;
        assert_eq!(history(&store), [(500, true), (750, false)]);
        store.replace(&SOURCE, &[])?;
        assert_eq!(history(&store), [(500, true), (750, true)]);
        store.replace(&SOURCE, &[offer(500)])?;
        assert_eq!(history(&store), [(500, true), (750, true), (500, false)]);
        let current: i64 = store
            .0
            .query_row("SELECT amount FROM offer", [], |r| r.get(0))?;
        assert_eq!(current, 500);
        Ok(())
    }

    #[test]
    fn tracks_metadata_changes_and_multiple_offers_per_merchant() -> Result<()> {
        let mut store = Store::open(":memory:")?;
        let mut changed = offer(500);
        store.replace(&SOURCE, &[changed.clone()])?;
        changed.category = Some("Shopping".into());
        store.replace(&SOURCE, &[changed.clone()])?;
        changed.up_to = true;
        store.replace(&SOURCE, &[changed.clone()])?;
        changed.kind = Kind::Fixed;
        store.replace(&SOURCE, &[changed.clone()])?;
        changed.name = "New name".into();
        store.replace(&SOURCE, &[changed.clone()])?;
        changed.url = "https://example.com/new".into();
        store.replace(&SOURCE, &[changed.clone(), offer(750)])?;
        store.replace(&SOURCE, &[changed, offer(750)])?;
        let rows = history(&store);
        assert_eq!(rows.len(), 7);
        assert_eq!(rows.iter().filter(|(_, ended)| !ended).count(), 2);
        assert!(rows[..5].iter().all(|(_, ended)| *ended));
        Ok(())
    }

    #[test]
    fn failures_and_other_sources_preserve_history() -> Result<()> {
        let mut store = Store::open(":memory:")?;
        let other = Source {
            name: "other",
            ..SOURCE
        };
        store.replace(&SOURCE, &[offer(500)])?;
        store.replace(&other, &[offer(750)])?;
        store
            .0
            .execute_batch("UPDATE offer_history SET last_seen_at = '2026-01-01 00:00:00'")?;
        store.fail(&SOURCE, "unavailable")?;
        assert_eq!(history(&store), [(500, false), (750, false)]);
        let untouched: i64 = store.0.query_row(
            "SELECT COUNT(*) FROM offer_history WHERE last_seen_at = '2026-01-01 00:00:00'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(untouched, 2);
        store.replace(&SOURCE, &[])?;
        assert_eq!(history(&store), [(500, true), (750, false)]);
        let other_last: String = store.0.query_row(
            "SELECT last_seen_at FROM offer_history WHERE source = 'other'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(other_last, "2026-01-01 00:00:00");
        Ok(())
    }

    #[test]
    fn migrates_existing_offers_once() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(
            "DROP TABLE offer_history;
             INSERT INTO source VALUES ('test', 'https://example.com', '2026-01-01 00:00:00', 1, NULL);
             INSERT INTO offer VALUES ('test', 'shop', 'Shop', 'percent', 500, 0, NULL, 'https://example.com/shop')",
        )?;
        let mut store = Store::initialize(conn)?;
        assert_eq!(history(&store), [(500, false)]);
        let first: String =
            store
                .0
                .query_row("SELECT first_seen_at FROM offer_history", [], |row| {
                    row.get(0)
                })?;
        assert_eq!(first, "2026-01-01 00:00:00");
        store.replace(&SOURCE, &[offer(750)])?;
        let store = Store::initialize(store.0)?;
        assert_eq!(history(&store), [(500, true), (750, false)]);
        Ok(())
    }

    #[test]
    fn history_write_failure_rolls_back_refresh() -> Result<()> {
        let mut store = Store::open(":memory:")?;
        store.replace(&SOURCE, &[offer(500)])?;
        store.fail(&SOURCE, "previous failure")?;
        store.0.execute_batch(
            "CREATE TRIGGER reject_history BEFORE INSERT ON offer_history
             BEGIN SELECT RAISE(ABORT, 'write failed'); END",
        )?;
        assert!(store.replace(&SOURCE, &[offer(750)]).is_err());
        assert_eq!(history(&store), [(500, false)]);
        let current: i64 = store
            .0
            .query_row("SELECT amount FROM offer", [], |r| r.get(0))?;
        assert_eq!(current, 500);
        let error: String = store
            .0
            .query_row("SELECT error FROM source", [], |r| r.get(0))?;
        assert_eq!(error, "previous failure");
        Ok(())
    }
}
