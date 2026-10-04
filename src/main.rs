use cashbackaren::store::Store;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "cashback.db".into());
    cashbackaren::run(&mut Store::open(&path)?)
}
