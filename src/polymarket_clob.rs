//! Polymarket CLOB V2 integration (https://clob.polymarket.com).
//!
//! Uses the official `polymarket_client_sdk_v2` for authenticated trading and
//! public REST endpoints for market data.

use crate::event::MarketPrices;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::str::FromStr;
use tracing::info;

pub const CLOB_HOST: &str = "https://clob.polymarket.com";
pub const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";

#[derive(Debug, Clone)]
pub struct TokenPair {
    pub yes_token_id: String,
    pub no_token_id: String,
}

#[derive(Debug, Deserialize)]
struct OrderBookSummary {
    bids: Vec<OrderLevel>,
    asks: Vec<OrderLevel>,
    #[serde(rename = "last_trade_price")]
    last_trade_price: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrderLevel {
    price: String,
    size: String,
}

#[derive(Debug, Deserialize)]
struct ClobMarketDetails {
    #[serde(rename = "t")]
    tokens: Option<Vec<ClobToken>>,
}

#[derive(Debug, Deserialize)]
struct ClobToken {
    #[serde(rename = "t")]
    token_id: String,
    #[serde(rename = "o")]
    outcome: String,
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.trim().is_empty())
}

fn parse_price(value: &str) -> Option<f64> {
    value.parse::<f64>().ok()
}

fn best_ask(book: &OrderBookSummary) -> Option<f64> {
    book.asks
        .first()
        .and_then(|level| parse_price(&level.price))
}

fn book_liquidity(book: &OrderBookSummary) -> f64 {
    book.asks
        .iter()
        .chain(book.bids.iter())
        .filter_map(|level| parse_price(&level.size))
        .sum()
}

fn clob_host() -> String {
    env("POLYMARKET_CLOB_HOST").unwrap_or_else(|| CLOB_HOST.to_string())
}

pub async fn fetch_order_book(http: &Client, token_id: &str) -> Result<OrderBookSummary> {
    let host = clob_host();
    let response = http
        .get(format!("{host}/book"))
        .query(&[("token_id", token_id)])
        .send()
        .await
        .with_context(|| format!("Failed to fetch order book for token {token_id}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "CLOB /book error {status} for token {token_id}: {body}"
        ));
    }

    response
        .json()
        .await
        .with_context(|| format!("Failed to parse order book for token {token_id}"))
}

pub async fn resolve_token_pair(http: &Client, condition_id: &str) -> Result<TokenPair> {
    let host = clob_host();
    let response = http
        .get(format!("{host}/clob-markets/{condition_id}"))
        .send()
        .await
        .with_context(|| format!("Failed to fetch CLOB market info for {condition_id}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "CLOB /clob-markets error {status} for {condition_id}: {body}"
        ));
    }

    let details: ClobMarketDetails = response
        .json()
        .await
        .with_context(|| format!("Failed to parse CLOB market info for {condition_id}"))?;

    let tokens = details
        .tokens
        .filter(|t| t.len() >= 2)
        .ok_or_else(|| anyhow::anyhow!("CLOB market {condition_id} has no token pair"))?;

    let mut yes_token_id = None;
    let mut no_token_id = None;

    for token in &tokens {
        let outcome = token.outcome.to_lowercase();
        if outcome == "yes" {
            yes_token_id = Some(token.token_id.clone());
        } else if outcome == "no" {
            no_token_id = Some(token.token_id.clone());
        }
    }

    if yes_token_id.is_none() || no_token_id.is_none() {
        // Gamma order: index 0 = Yes, index 1 = No
        let yes = tokens[0].token_id.clone();
        let no = tokens[1].token_id.clone();
        return Ok(TokenPair {
            yes_token_id: yes,
            no_token_id: no,
        });
    }

    Ok(TokenPair {
        yes_token_id: yes_token_id.unwrap(),
        no_token_id: no_token_id.unwrap(),
    })
}

pub async fn fetch_prices_for_tokens(
    http: &Client,
    yes_token_id: &str,
    no_token_id: &str,
) -> Result<MarketPrices> {
    let (yes_book, no_book) = tokio::join!(
        fetch_order_book(http, yes_token_id),
        fetch_order_book(http, no_token_id),
    );

    let yes_book = yes_book?;
    let no_book = no_book?;

    let yes_ask = best_ask(&yes_book).unwrap_or(0.0);
    let no_ask = best_ask(&no_book).unwrap_or(0.0);

    let liquidity = book_liquidity(&yes_book) + book_liquidity(&no_book);
    let last_price = yes_book
        .last_trade_price
        .as_deref()
        .and_then(parse_price)
        .or_else(|| no_book.last_trade_price.as_deref().and_then(parse_price));

    // yes/no store best ask — the price to buy each side on CLOB V2.
    Ok(MarketPrices::new(yes_ask, no_ask, liquidity).with_asks(yes_ask, no_ask, last_price))
}

pub fn parse_clob_token_ids(raw: Option<&str>) -> Option<TokenPair> {
    let raw = raw?;
    if let Ok(ids) = serde_json::from_str::<Vec<String>>(raw) {
        if ids.len() >= 2 {
            return Some(TokenPair {
                yes_token_id: ids[0].clone(),
                no_token_id: ids[1].clone(),
            });
        }
    }
    None
}

/// Gamma returns `clobTokenIds` as a JSON string or array.
pub fn parse_clob_token_ids_from_market(market: &serde_json::Value) -> Option<TokenPair> {
    let field = market.get("clobTokenIds")?;
    if let Some(raw) = field.as_str() {
        return parse_clob_token_ids(Some(raw));
    }
    if let Some(arr) = field.as_array() {
        let ids: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        if ids.len() >= 2 {
            return Some(TokenPair {
                yes_token_id: ids[0].clone(),
                no_token_id: ids[1].clone(),
            });
        }
    }
    None
}

pub async fn place_clob_order(
    condition_id: &str,
    outcome: &str,
    amount_usd: f64,
    max_price: f64,
    yes_token_id: Option<&str>,
    no_token_id: Option<&str>,
) -> Result<Option<String>> {
    if env("DRY_RUN")
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        info!(
            "[DRY RUN] Would place Polymarket CLOB order: condition={} outcome={} amount={} max_price={}",
            condition_id, outcome, amount_usd, max_price
        );
        return Ok(Some("dry-run".to_string()));
    }

    let private_key = env("POLYMARKET_WALLET_PRIVATE_KEY")
        .or_else(|| env("POLYMARKET_PRIVATE_KEY"))
        .context(
            "Polymarket private key required (POLYMARKET_WALLET_PRIVATE_KEY or POLYMARKET_PRIVATE_KEY)",
        )?;

    let http = Client::new();
    let tokens = match (yes_token_id, no_token_id) {
        (Some(yes), Some(no)) => TokenPair {
            yes_token_id: yes.to_string(),
            no_token_id: no.to_string(),
        },
        _ => resolve_token_pair(&http, condition_id).await?,
    };

    let token_id = match outcome.to_uppercase().as_str() {
        "YES" => tokens.yes_token_id,
        "NO" => tokens.no_token_id,
        other => return Err(anyhow::anyhow!("Invalid Polymarket outcome: {other}")),
    };

    if max_price <= 0.0 {
        return Err(anyhow::anyhow!("Invalid max price: {max_price}"));
    }

    let shares = amount_usd / max_price;
    if shares <= 0.0 {
        return Err(anyhow::anyhow!("Order size too small for amount {amount_usd}"));
    }

    use alloy::signers::local::LocalSigner;
    use polymarket_client_sdk_v2::clob::types::Side;
    use polymarket_client_sdk_v2::clob::{Client, Config};
    use polymarket_client_sdk_v2::types::{Decimal, U256, POLYGON};
    use polymarket_client_sdk_v2::PRIVATE_KEY_VAR;

    let _ = PRIVATE_KEY_VAR; // documented SDK env name; we accept both keys above.

    let signer = LocalSigner::from_str(&private_key)
        .with_context(|| "Invalid Polymarket private key format")?
        .with_chain_id(Some(POLYGON));

    let clob_host = env("POLYMARKET_CLOB_HOST").unwrap_or_else(|| CLOB_HOST.to_string());

    let mut auth = Client::new(clob_host, Config::default())?
        .authentication_builder(&signer);

    if let Some(funder) = env("POLYMARKET_FUNDER_ADDRESS")
        .or_else(|| env("DEPOSIT_WALLET_ADDRESS"))
    {
        let funder = funder
            .parse()
            .with_context(|| format!("Invalid POLYMARKET_FUNDER_ADDRESS: {funder}"))?;
        auth = auth.funder(funder);
    }

    let client = auth
        .signature_type(signature_type_from_env())
        .authenticate()
        .await
        .context("Failed to authenticate Polymarket CLOB client (L1/L2)")?;

    let token = U256::from_str(&token_id)
        .with_context(|| format!("Invalid Polymarket token id: {token_id}"))?;

    let size = Decimal::from_f64_retain(shares)
        .with_context(|| format!("Invalid order size: {shares}"))?;
    let price = Decimal::from_f64_retain(max_price)
        .with_context(|| format!("Invalid order price: {max_price}"))?;

    // Limit buy at max_price — fills immediately when ask <= max_price.
    let order = client
        .limit_order()
        .token_id(token)
        .size(size)
        .price(price)
        .side(Side::Buy)
        .build()
        .await
        .context("Failed to build Polymarket CLOB V2 order")?;

    let signed_order = client
        .sign(&signer, order)
        .await
        .context("Failed to sign Polymarket CLOB V2 order")?;

    let response = client
        .post_order(signed_order)
        .await
        .context("Failed to post Polymarket CLOB V2 order")?;

    info!(
        "Polymarket CLOB order posted: id={} status={:?}",
        response.order_id, response.status
    );

    Ok(Some(response.order_id))
}

fn signature_type_from_env() -> polymarket_client_sdk_v2::clob::types::SignatureType {
    use polymarket_client_sdk_v2::clob::types::SignatureType;

    match env("POLYMARKET_SIGNATURE_TYPE")
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "1" | "magic" | "proxy" | "email" => SignatureType::Proxy,
        "2" | "safe" | "gnosis" | "browser" => SignatureType::GnosisSafe,
        "3" | "poly1271" | "deposit" | "poly_1271" => SignatureType::Poly1271,
        _ => SignatureType::Eoa,
    }
}
