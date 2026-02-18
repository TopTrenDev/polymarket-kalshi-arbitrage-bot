use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub platform: String,
    pub event_id: String,
    pub title: String,
    pub description: String,
    pub resolution_date: Option<DateTime<Utc>>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub slug: Option<String>,
}

impl Event {
    pub fn new(
        platform: String,
        event_id: String,
        title: String,
        description: String,
    ) -> Self {
        Self {
            platform,
            event_id,
            title,
            description,
            resolution_date: None,
            category: None,
            tags: Vec::new(),
            slug: None,
        }
    }

    pub fn with_resolution_date(mut self, date: DateTime<Utc>) -> Self {
        self.resolution_date = Some(date);
        self
    }

    pub fn with_category(mut self, category: String) -> Self {
        self.category = Some(category);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_slug(mut self, slug: String) -> Self {
        self.slug = Some(slug);
        self
    }

    pub fn slug_is_15m_crypto(&self) -> bool {
        self.slug
            .as_deref()
            .map(|s| s.contains("updown-15m"))
            .unwrap_or(false)
    }

    fn ticker_looks_15m_crypto(ticker: &str) -> bool {
        let lower = ticker.to_lowercase();
        let has_15m = lower.contains("15m");
        let has_coin = lower.contains("btc")
            || lower.contains("eth")
            || lower.contains("sol")
            || lower.contains("bitcoin")
            || lower.contains("ethereum")
            || lower.contains("solana");
        has_15m && has_coin
    }

    pub fn is_15m_crypto_market(&self) -> bool {
        if self.slug_is_15m_crypto() {
            return true;
        }
        let ticker = self.slug.as_deref().unwrap_or(&self.event_id);
        self.platform == "kalshi" && Self::ticker_looks_15m_crypto(ticker)
    }

    pub fn coin_from_slug(&self) -> Option<String> {
        if let Some(slug) = self.slug.as_deref() {
            if slug.contains("updown-15m") {
                let prefix = slug.split("-updown-15m").next()?;
                if !prefix.is_empty() {
                    return Some(prefix.to_lowercase());
                }
            }
        }
        let ticker = self.slug.as_deref().unwrap_or(&self.event_id).to_lowercase();
        if ticker.contains("btc") || ticker.contains("bitcoin") {
            return Some("btc".to_string());
        }
        if ticker.contains("eth") || ticker.contains("ethereum") {
            return Some("eth".to_string());
        }
        if ticker.contains("sol") || ticker.contains("solana") {
            return Some("sol".to_string());
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct MarketPrices {
    pub yes: f64,
    pub no: f64,
    pub liquidity: f64,
}

impl MarketPrices {
    pub fn new(yes: f64, no: f64, liquidity: f64) -> Self {
        Self {
            yes,
            no,
            liquidity,
        }
    }

    pub fn validate(&self) -> bool {
        (self.yes + self.no - 1.0).abs() < 0.01
    }
}

