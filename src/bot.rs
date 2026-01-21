use crate::arbitrage_detector::{ArbitrageDetector, ArbitrageOpportunity};
use crate::event::{Event, MarketPrices};
use crate::event_matcher::EventMatcher;
use crate::gabagool_detector::{GabagoolDetector, GabagoolOpportunity};
use chrono::{DateTime, Duration, Utc};
use std::time::Duration as StdDuration;
use tokio::time;

pub struct MarketFilters {
    pub categories: Vec<String>,
    pub max_hours_until_resolution: i64,
    pub min_liquidity: f64,
}

impl Default for MarketFilters {
    fn default() -> Self {
        Self {
            categories: vec!["crypto".to_string(), "sports".to_string()],
            max_hours_until_resolution: 24,
            min_liquidity: 100.0,
        }
    }
}

pub struct ShortTermArbitrageBot {
    filters: MarketFilters,
    event_matcher: EventMatcher,
    arbitrage_detector: ArbitrageDetector,
    gabagool_detector: GabagoolDetector,
}

impl ShortTermArbitrageBot {
    pub fn new(
        filters: MarketFilters,
        similarity_threshold: f64,
        min_profit_threshold: f64,
    ) -> Self {
        Self {
            filters,
            event_matcher: EventMatcher::new(similarity_threshold),
            arbitrage_detector: ArbitrageDetector::new(min_profit_threshold),
            gabagool_detector: GabagoolDetector::new(min_profit_threshold),
        }
    }

    pub fn is_within_timeframe(&self, resolution_date: Option<DateTime<Utc>>) -> bool {
        if let Some(date) = resolution_date {
            let now = Utc::now();
            let time_until_resolution = date - now;
            let max_time = Duration::hours(self.filters.max_hours_until_resolution);
            let min_time = Duration::minutes(5);

            time_until_resolution >= min_time && time_until_resolution <= max_time
        } else {
            false
        }
    }

    pub fn matches_category(&self, event: &Event) -> bool {
        if self.filters.categories.is_empty() {
            return true;
        }

        if let Some(ref cat) = event.category {
            let cat_lower = cat.to_lowercase();
            for filter_cat in &self.filters.categories {
                if cat_lower.contains(&filter_cat.to_lowercase()) {
                    return true;
                }
            }
        }

        let title_lower = event.title.to_lowercase();
        let desc_lower = event.description.to_lowercase();

        let crypto_keywords = [
            "bitcoin", "btc", "ethereum", "eth", "crypto", "cryptocurrency",
            "price", "above", "below", "reach", "hit", "surpass",
        ];

        let sports_keywords = [
            "game", "match", "team", "player", "score", "win", "lose",
            "nfl", "nba", "mlb", "soccer", "football", "basketball",
        ];

        let has_crypto = self.filters.categories.iter().any(|c| c == "crypto");
        let has_sports = self.filters.categories.iter().any(|c| c == "sports");

        if has_crypto {
            if crypto_keywords.iter().any(|kw| title_lower.contains(kw) || desc_lower.contains(kw)) {
                return true;
            }
        }

        if has_sports {
            if sports_keywords.iter().any(|kw| title_lower.contains(kw) || desc_lower.contains(kw)) {
                return true;
            }
        }

        false
    }

    pub fn filter_events(&self, events: &[Event]) -> Vec<Event> {
        events
            .iter()
            .filter(|event| {
                self.matches_category(event) && self.is_within_timeframe(event.resolution_date)
            })
            .cloned()
            .collect()
    }

    pub async fn scan_for_opportunities<F, Fut>(
        &self,
        pm_events: &[Event],
        kalshi_events: &[Event],
        fetch_prices: F,
    ) -> Vec<(Event, Event, ArbitrageOpportunity)>
    where
        F: Fn(&str, &str) -> Fut,
        Fut: std::future::Future<Output = MarketPrices> + Send,
    {

        let pm_filtered = self.filter_events(pm_events);
        let kalshi_filtered = self.filter_events(kalshi_events);

        if pm_filtered.is_empty() || kalshi_filtered.is_empty() {
            return Vec::new();
        }

        let matches = self.event_matcher.find_matches(&pm_filtered, &kalshi_filtered);

        if matches.is_empty() {
            return Vec::new();
        }

        let price_futures: Vec<_> = matches
            .iter()
            .map(|(pm_event, kalshi_event, _)| {
                let pm_id = pm_event.event_id.clone();
                let kalshi_id = kalshi_event.event_id.clone();
                let pm_event_clone = pm_event.clone();
                let kalshi_event_clone = kalshi_event.clone();
                async move {
                    let (pm_prices, kalshi_prices) = tokio::join!(
                        fetch_prices(&pm_id, "polymarket"),
                        fetch_prices(&kalshi_id, "kalshi")
                    );
                    (pm_event_clone, kalshi_event_clone, pm_prices, kalshi_prices)
                }
            })
            .collect();

        let price_results = futures::future::join_all(price_futures).await;

        let mut opportunities = Vec::new();

        for (pm_event, kalshi_event, pm_prices, kalshi_prices) in price_results {
            if pm_prices.liquidity < self.filters.min_liquidity
                || kalshi_prices.liquidity < self.filters.min_liquidity
            {
                continue;
            }

            if let Some(opportunity) = self.arbitrage_detector.check_arbitrage(&pm_prices, &kalshi_prices) {
                opportunities.push((pm_event, kalshi_event, opportunity));
            }
        }

        opportunities
    }

    pub async fn scan_gabagool_opportunities<F, Fut, G, Gfut>(
        &self,
        pm_events: &[Event],
        fetch_prices: F,
        get_position_balance: G,
    ) -> Vec<GabagoolOpportunity>
    where
        F: Fn(&str) -> Fut,
        Fut: std::future::Future<Output = MarketPrices> + Send,
        G: Fn(&str) -> Gfut,
        Gfut: std::future::Future<Output = (f64, f64, f64, f64)> + Send,
    {

        let pm_filtered = self.filter_events(pm_events);

        if pm_filtered.is_empty() {
            return Vec::new();
        }

        let opportunity_futures: Vec<_> = pm_filtered
            .iter()
            .map(|event| {
                let event_id = event.event_id.clone();
                let event_clone = event.clone();
                async move {
                    let (prices, (yes_qty, yes_cost, no_qty, no_cost)) = tokio::join!(
                        fetch_prices(&event_id),
                        get_position_balance(&event_id)
                    );
                    (event_clone, prices, yes_qty, yes_cost, no_qty, no_cost)
                }
            })
            .collect();

        let results = futures::future::join_all(opportunity_futures).await;

        let mut opportunities = Vec::new();

        for (event, prices, yes_qty, yes_cost, no_qty, no_cost) in results {
            if prices.liquidity < self.filters.min_liquidity {
                continue;
            }

            if let Some(opportunity) = self.gabagool_detector.check_opportunity(
                &event,
                &prices,
                yes_qty,
                no_qty,
                yes_cost,
                no_cost,
            ) {
                opportunities.push(opportunity);
            }
        }

        opportunities
    }

    pub async fn run_continuous<F, Fut, P, PFut>(
        &self,
        scan_interval: StdDuration,
        fetch_events: F,
        fetch_prices: P,
    ) -> Vec<(Event, Event, ArbitrageOpportunity)>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = (Vec<Event>, Vec<Event>)> + Send,
        P: Fn(&str, &str) -> PFut + Clone + Send + Sync,
        PFut: std::future::Future<Output = MarketPrices> + Send,
    {
        let mut interval = time::interval(scan_interval);

        loop {
            interval.tick().await;

            let (pm_events, kalshi_events) = fetch_events().await;
            let opportunities = self.scan_for_opportunities(&pm_events, &kalshi_events, fetch_prices.clone()).await;

            if !opportunities.is_empty() {
                tracing::info!("Found {} arbitrage opportunities", opportunities.len());
                for (pm_event, kalshi_event, opp) in &opportunities {
                    tracing::info!(
                        "Opportunity: {} - Profit: ${:.4}, ROI: {:.2}%",
                        pm_event.title,
                        opp.net_profit,
                        opp.roi_percent
                    );
                }
                return opportunities;
            }
        }
    }
}

