use crate::event::{Event, MarketPrices};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

struct PriceCacheEntry {
    prices: MarketPrices,
    timestamp: Instant,
}

struct PriceCache {
    entries: Arc<RwLock<std::collections::HashMap<String, PriceCacheEntry>>>,
    ttl: Duration,
}

impl PriceCache {
    fn new(ttl_secs: u64) -> Self {
        Self {
            entries: Arc::new(RwLock::new(std::collections::HashMap::new())),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    async fn get(&self, key: &str) -> Option<MarketPrices> {
        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(key) {
            if entry.timestamp.elapsed() < self.ttl {
                return Some(entry.prices.clone());
            }
        }
        None
    }

    async fn set(&self, key: String, prices: MarketPrices) {
        let mut entries = self.entries.write().await;
        entries.insert(key, PriceCacheEntry {
            prices,
            timestamp: Instant::now(),
        });
    }
}

#[derive(Clone)]
pub struct PolymarketClient {
    http_client: Client,
    polygon_rpc_url: String,
    wallet_private_key: Option<String>,
    base_url: String,
    price_cache: Arc<PriceCache>,
}

impl PolymarketClient {
    pub fn new() -> Self {

        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            http_client,
            polygon_rpc_url: std::env::var("POLYGON_RPC_URL")
                .unwrap_or_else(|_| "https:
            wallet_private_key: std::env::var("POLYMARKET_WALLET_PRIVATE_KEY").ok(),
            base_url: "https:
        }
    }

    pub fn with_wallet(mut self, private_key: String) -> Self {
        self.wallet_private_key = Some(private_key);
        self
    }

    pub fn with_rpc(mut self, rpc_url: String) -> Self {
        self.polygon_rpc_url = rpc_url;
        self
    }

    pub async fn fetch_events(&self) -> Result<Vec<Event>> {
        let use_gamma = std::env::var("POLYMARKET_USE_GAMMA")
            .unwrap_or_else(|_| "1".to_string());
        if use_gamma == "1" || use_gamma.eq_ignore_ascii_case("true") {
            let tag_slug = std::env::var("POLYMARKET_TAG_SLUG").ok();
            let tag_slug = tag_slug.as_deref().filter(|s| !s.is_empty());
            if let Ok(events) = self
                .fetch_events_from_gamma(tag_slug, 200)
                .await
            {
                return Ok(events);
            }
            tracing::warn!("Gamma API fetch failed, falling back to GraphQL");
        }

        let query = r#"
            query GetMarkets($active: Boolean) {
                markets(active: $active, limit: 1000) {
                    id
                    question
                    description
                    endDate
                    category
                    outcomes {
                        title
                        price
                    }
                }
            }
        "#;

        let variables = serde_json::json!({
            "active": true
        });

        let response = self
            .http_client
            .post(&format!("{}/graphql", self.base_url))
            .json(&serde_json::json!({
                "query": query,
                "variables": variables
            }))
            .send()
            .await
            .context("Failed to fetch Polymarket events")?;

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse Polymarket response")?;

        let mut events = Vec::new();

        if let Some(markets) = data["data"]["markets"].as_array() {
            for market in markets {
                let event_id = market["id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let title = market["question"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let description = market["description"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let category = market["category"]
                    .as_str()
                    .map(|s| s.to_string());

                let resolution_date = market["endDate"]
                    .as_str()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                events.push(Event {
                    platform: "polymarket".to_string(),
                    event_id,
                    title,
                    description,
                    resolution_date,
                    category,
                    tags: Vec::new(),
                    slug: None,
                });
            }
        }

        Ok(events)
    }

    const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";

    pub async fn fetch_events_from_gamma(
        &self,
        tag_slug: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Event>> {
        let limit = limit.min(200);
        let mut query = vec![
            ("active", "true"),
            ("closed", "false"),
            ("limit", limit.to_string()),
        ];
        if let Some(t) = tag_slug {
            if !t.is_empty() {
                query.push(("tag_slug", t));
            }
        }

        let url = format!("{}/events", Self::GAMMA_API_BASE);
        let response = self
            .http_client
            .get(&url)
            .query(&query)
            .send()
            .await
            .context("Failed to fetch Polymarket events from Gamma API")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Gamma API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            ));
        }

        let data: Vec<serde_json::Value> = response
            .json()
            .await
            .context("Failed to parse Gamma API response")?;

        let mut events = Vec::new();
        for event_data in data {
            let slug = event_data["slug"].as_str().map(|s| s.to_string());
            let title = event_data["title"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let description = event_data["subtitle"]
                .as_str()
                .unwrap_or(event_data["description"].as_str().unwrap_or(""))
                .to_string();
            let resolution_date = event_data["endDate"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc));
            let category = event_data["category"].as_str().map(|s| s.to_string());

            let tags: Vec<String> = event_data["tags"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t["slug"].as_str().or_else(|| t["label"].as_str()))
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();

            let markets = event_data["markets"].as_array();
            let event_id = markets
                .and_then(|m| m.first())
                .and_then(|m| m["conditionId"].as_str().or_else(|| m["id"].as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    event_data["id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string()
                });

            events.push(Event {
                platform: "polymarket".to_string(),
                event_id,
                title,
                description,
                resolution_date,
                category,
                tags,
                slug,
            });
        }

        Ok(events)
    }

    pub async fn fetch_prices(&self, event_id: &str) -> Result<MarketPrices> {
        if let Some(cached) = self.price_cache.get(event_id).await {
            return Ok(cached);
        }

        let url = format!("https://clob.polymarket.com/clob/v1/book");
        
        let response = self
            .http_client
            .get(&url)
            .query(&[("market", event_id)])
            .send()
            .await
            .context("Failed to fetch Polymarket prices")?;

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse price response")?;

        let yes_price = data["yes"]
            .as_object()
            .and_then(|o| o.get("bestBid"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let no_price = data["no"]
            .as_object()
            .and_then(|o| o.get("bestBid"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let liquidity = data["liquidity"]
            .as_f64()
            .unwrap_or(0.0);

        let prices = MarketPrices::new(yes_price, no_price, liquidity);
        self.price_cache.set(event_id.to_string(), prices.clone()).await;
        Ok(prices)
    }

    pub async fn place_order(
        &self,
        event_id: String,
        outcome: String,
        amount: f64,
        max_price: f64,
    ) -> Result<Option<String>> {

        let private_key = self
            .wallet_private_key
            .as_ref()
            .context("Polymarket wallet private key not configured. Set POLYMARKET_WALLET_PRIVATE_KEY environment variable")?;

        use crate::polymarket_blockchain::PolymarketBlockchain;
        
        let blockchain = PolymarketBlockchain::new(&self.polygon_rpc_url)?
            .with_wallet(private_key)
            .context("Failed to initialize blockchain client")?;

        match blockchain.place_order_via_blockchain(&event_id, &outcome, amount, max_price).await {
            Ok(Some(tx_hash)) => {
                info!("Polymarket order placed via blockchain: {}", tx_hash);
                Ok(Some(tx_hash))
            }
            Ok(None) => {
                warn!("Polymarket order returned None (may need contract addresses)");
                Err(anyhow::anyhow!("Order placement failed - contract addresses may be missing"))
            }
            Err(e) => {
                warn!("Blockchain order failed: {:?}. Attempting CLOB API...", e);

                blockchain.place_order_via_clob(&self.http_client, &event_id, &outcome, amount, max_price).await
            }
        }
    }

    pub async fn check_settlement(&self, event_id: &str) -> Result<Option<bool>> {

        let query = r#"
            query GetMarket($id: ID!) {
                market(id: $id) {
                    resolved
                    outcome
                }
            }
        "#;

        let variables = serde_json::json!({
            "id": event_id
        });

        let response = self
            .http_client
            .post(&format!("{}/graphql", self.base_url))
            .json(&serde_json::json!({
                "query": query,
                "variables": variables
            }))
            .send()
            .await
            .context("Failed to check Polymarket settlement")?;

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse settlement response")?;

        if let Some(resolved) = data["data"]["market"]["resolved"].as_bool() {
            if resolved {
                if let Some(outcome) = data["data"]["market"]["outcome"].as_str() {
                    return Ok(Some(outcome == "YES"));
                }
            }
        }

        Ok(None)
    }

    pub async fn get_balance(&self) -> Result<f64> {
        let private_key = self
            .wallet_private_key
            .as_ref()
            .context("Wallet private key required for balance check")?;

        use crate::polymarket_blockchain::PolymarketBlockchain;
        
        let blockchain = PolymarketBlockchain::new(&self.polygon_rpc_url)?
            .with_wallet(private_key)
            .context("Failed to initialize blockchain client")?;

        blockchain.get_usdc_balance().await
    }
}

#[derive(Clone)]
pub struct KalshiClient {
    http_client: Client,
    api_id: String,        // Kalshi API ID (sent in X-API-KEY header)
    rsa_private_key: String, // RSA private key for signing (PEM format)
    base_url: String,
    price_cache: Arc<PriceCache>,
}

impl KalshiClient {
    /// Creates a new KalshiClient
    /// 
    /// # Arguments
    /// * `api_id` - Your Kalshi API ID (not a traditional "key", this is your account identifier)
    /// * `rsa_private_key` - Your RSA private key in PEM format (PKCS1 or PKCS8)
    /// 
    /// # Note
    /// Kalshi uses RSA-PSS signing for authentication. The API ID goes in X-API-KEY header,
    /// and the RSA private key is used to sign requests with SHA256.
    pub fn new(api_id: String, rsa_private_key: String) -> Self {

        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            http_client,
            api_id,
            rsa_private_key,
            base_url: "https:
        }
    }


    fn get_auth_headers(&self, method: &str, path: &str, body: &str) -> Result<reqwest::header::HeaderMap> {
        use reqwest::header::{HeaderMap, HeaderValue};
        use std::time::{SystemTime, UNIX_EPOCH};
        use rsa::{RsaPrivateKey, pkcs1v15::{SigningKey, VerifyingKey}};
        use rsa::signature::{Signer, Verifier};
        use sha2::Sha256;
        use base64::{engine::general_purpose, Engine as _};

        let mut headers = HeaderMap::new();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();

        let signature_string = format!("{}\n{}\n{}\n{}", timestamp, method, path, body);


        // Parse RSA private key (supports both PKCS8 and PKCS1 PEM formats)
        let signature_b64 = if let Ok(private_key) = RsaPrivateKey::from_pkcs8_pem(&self.rsa_private_key) {
            let signing_key = SigningKey::<Sha256>::new(private_key);
            let signature = signing_key.sign(signature_string.as_bytes());
            general_purpose::STANDARD.encode(&signature.to_bytes())
        } else if let Ok(private_key) = RsaPrivateKey::from_pkcs1_pem(&self.rsa_private_key) {
            let signing_key = SigningKey::<Sha256>::new(private_key);
            let signature = signing_key.sign(signature_string.as_bytes());
            general_purpose::STANDARD.encode(&signature.to_bytes())
        } else {
            warn!("Failed to parse RSA private key. Expected PEM format (PKCS1 or PKCS8). Authentication may fail.");
            String::new()
        };

        // Kalshi uses X-API-KEY header with the API ID (not a traditional API key)
        headers.insert(
            "X-API-KEY",
            HeaderValue::from_str(&self.api_id)
                .context("Invalid API ID")?,
        );
        
        headers.insert(
            "X-TIMESTAMP",
            HeaderValue::from_str(&timestamp)
                .context("Invalid timestamp")?,
        );
        
        if !signature_b64.is_empty() {
            headers.insert(
                "X-SIGNATURE",
                HeaderValue::from_str(&signature_b64)
                    .context("Invalid signature")?,
            );
        }
        
        headers.insert(
            "Content-Type",
            HeaderValue::from_static("application/json"),
        );

        Ok(headers)
    }

    pub async fn fetch_events(&self) -> Result<Vec<Event>> {
        let path = "/trade-api/v2/events";
        let headers = self.get_auth_headers("GET", path, "")?;
        let query_params = self.events_query_params();

        let response = self
            .http_client
            .get(&format!("{}{}", self.base_url, path))
            .headers(headers)
            .query(&query_params)
            .send()
            .await
            .context("Failed to fetch Kalshi events")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Kalshi API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            ));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse Kalshi response")?;

        let mut events = Vec::new();

        if let Some(events_array) = data["events"].as_array() {
            for event_data in events_array {
                let event_ticker = event_data["event_ticker"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let title = event_data["title"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let subtitle = event_data["subtitle"]
                    .as_str()
                    .or_else(|| event_data["sub_title"].as_str())
                    .unwrap_or("")
                    .to_string();
                let category = event_data["category"]
                    .as_str()
                    .map(|s| s.to_string());

                let resolution_date = event_data["expected_expiration_time"]
                    .as_str()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                let series_ticker = event_data["series_ticker"]
                    .as_str()
                    .map(|s| s.to_string());
                let tags = series_ticker.into_iter().collect::<Vec<_>>();

                events.push(Event {
                    platform: "kalshi".to_string(),
                    event_id: event_ticker.clone(),
                    title,
                    description: subtitle,
                    resolution_date,
                    category,
                    tags,
                    slug: Some(event_ticker),
                });
            }
        }

        Ok(events)
    }

    fn events_query_params(&self) -> Vec<(&'static str, String)> {
        let mut params = vec![
            ("status", "open".to_string()),
            ("limit", "200".to_string()),
        ];
        if let Ok(st) = std::env::var("KALSHI_SERIES_TICKER") {
            if !st.is_empty() {
                params.push(("series_ticker", st));
            }
        }
        params
    }

    pub async fn fetch_prices(&self, event_id: &str) -> Result<MarketPrices> {
        if let Some(cached) = self.price_cache.get(event_id).await {
            return Ok(cached);
        }

        let path = format!("/trade-api/v2/events/{}/markets", event_id);
        let headers = self.get_auth_headers("GET", &path, "")?;

        let response = self
            .http_client
            .get(&format!("{}{}", self.base_url, path))
            .headers(headers)
            .send()
            .await
            .context("Failed to fetch Kalshi prices")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Kalshi API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            ));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse Kalshi price response")?;

        let mut yes_price = 0.0;
        let mut no_price = 0.0;
        let mut liquidity = 0.0;

        if let Some(markets) = data["markets"].as_array() {
            for market in markets {
                let subtitle = market["subtitle"].as_str().unwrap_or("");
                let last_price = market["last_price"]
                    .as_i64()
                    .unwrap_or(0) as f64
                    / 100.0;

                if subtitle == "Yes" {
                    yes_price = last_price;
                } else if subtitle == "No" {
                    no_price = last_price;
                }

                if let Some(vol) = market["volume"].as_f64() {
                    liquidity += vol;
                }
            }
        }

        let prices = MarketPrices::new(yes_price, no_price, liquidity);
        self.price_cache.set(event_id.to_string(), prices.clone()).await;
        Ok(prices)
    }

    pub async fn place_order(
        &self,
        event_id: String,
        outcome: String,
        amount: f64,
        price: f64,
    ) -> Result<Option<String>> {
        let path = "/trade-api/v2/orders";

        let order_data = serde_json::json!({
            "event_ticker": event_id,
            "side": "buy",
            "outcome": outcome,
            "count": (amount / price) as i64,
            "price": (price * 100) as i64,
        });

        let body = serde_json::to_string(&order_data)?;
        let headers = self.get_auth_headers("POST", path, &body)?;

        let response = self
            .http_client
            .post(&format!("{}{}", self.base_url, path))
            .headers(headers)
            .json(&order_data)
            .send()
            .await
            .context("Failed to place Kalshi order")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Kalshi order failed: {} - {}",
                response.status(),
                error_text
            ));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse Kalshi order response")?;

        let order_id = data["order"]["order_id"]
            .as_str()
            .map(|s| s.to_string());

        Ok(order_id)
    }

    pub async fn check_settlement(&self, event_id: &str) -> Result<Option<bool>> {
        let path = format!("/trade-api/v2/events/{}", event_id);
        let headers = self.get_auth_headers("GET", &path, "")?;

        let response = self
            .http_client
            .get(&format!("{}{}", self.base_url, path))
            .headers(headers)
            .send()
            .await
            .context("Failed to check Kalshi settlement")?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse settlement response")?;

        if let Some(status) = data["event"]["status"].as_str() {
            if status == "resolved" {

                if let Some(outcome) = data["event"]["outcome"].as_str() {
                    return Ok(Some(outcome == "Yes" || outcome == "YES"));
                }
            }
        }

        Ok(None)
    }

    pub async fn get_balance(&self) -> Result<f64> {
        let path = "/trade-api/v2/portfolio/balance";
        let headers = self.get_auth_headers("GET", path, "")?;

        let response = self
            .http_client
            .get(&format!("{}{}", self.base_url, path))
            .headers(headers)
            .send()
            .await
            .context("Failed to fetch Kalshi balance")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Kalshi balance check failed: {}",
                response.status()
            ));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse balance response")?;

        let balance = data["balance"]
            .as_f64()
            .or_else(|| data["balance"].as_str().and_then(|s| s.parse().ok()))
            .unwrap_or(0.0);

        Ok(balance)
    }
}
