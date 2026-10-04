#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Percent,
    Fixed,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Percent => "percent",
            Kind::Fixed => "fixed",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    // The store's domain, shared across sources.
    pub merchant: String,
    pub name: String,
    pub kind: Kind,
    // Basis points for Percent, öre for Fixed.
    pub amount: i64,
    pub up_to: bool,
    pub category: Option<String>,
    pub url: String,
}
