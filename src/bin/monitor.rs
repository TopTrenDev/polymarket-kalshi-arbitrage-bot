use anyhow::Result;
use chrono::Utc;
use polymarket_kalshi_arbitrage_bot::{
    config::KalshiConfig,
    clients::KalshiClient,
    event::MarketPrices,
    monitor_logger::append_monitor_log,
};
use std::time::Duration;
use tracing::{error, info, Level};

const DEFAULT_INTERVAL_MS: u64 = 2000;
const BTC_SERIES_TICKER: &str = "KXBTC15M";

fn format_prices_line(ticker: &str, p: &MarketPrices) -> String {
    let up_ask = p.yes_ask_or_fallback();
    let down_ask = p.no_ask_or_fallback();
    let last = p.last_price.unwrap_or((up_ask + down_ask) * 0.5);
    format!(
        "UP ask={:.2}  |  DOWN ask={:.2}  |  last={:.2}  @ {}",
        up_ask,
        down_ask,
        last,
        Utc::now().to_rfc3339()
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    dotenv::dotenv().ok();

    let kalshi_config = KalshiConfig::from_env();
    if kalshi_config.api_id.is_empty() || kalshi_config.rsa_private_key.is_empty() {
        error!("Kalshi credentials required (KALSHI_API_ID, KALSHI_RSA_PRIVATE_KEY or KALSHI_PRIVATE_KEY_PATH)");
        return Err(anyhow::anyhow!("Missing Kalshi API credentials"));
    }

    let client = KalshiClient::from_config(&kalshi_config);
    let interval_ms = std::env::var("KALSHI_MONITOR_INTERVAL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_INTERVAL_MS);
    let ticker_override = std::env::var("KALSHI_MONITOR_TICKER").ok();

    let mut ticker = ticker_override.clone().unwrap_or_else(|| {
        info!("No KALSHI_MONITOR_TICKER set, fetching first open {} market...", BTC_SERIES_TICKER);
        String::new()
    });

    if ticker.is_empty() {
        let tickers = client
            .fetch_open_market_tickers(BTC_SERIES_TICKER)
            .await?;
        ticker = tickers
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No open {} markets found", BTC_SERIES_TICKER))?;
        info!("Using market: {}", ticker);
    }

    info!(
        "Starting price monitor (poll every {}ms, ticker={})",
        interval_ms, ticker
    );

    let mut last_slot = polymarket_kalshi_arbitrage_bot::monitor_logger::time_bucket_15m(&Utc::now());
    let cancel = tokio::signal::ctrl_c();
    tokio::pin!(cancel);

    loop {
        let now = Utc::now();
        let slot = polymarket_kalshi_arbitrage_bot::monitor_logger::time_bucket_15m(&now);

        if slot != last_slot && ticker_override.is_none() {
            last_slot = slot.clone();
            if let Ok(tickers) = client.fetch_open_market_tickers(BTC_SERIES_TICKER).await {
                if let Some(first) = tickers.into_iter().next() {
                    ticker = first;
                    info!("New 15m slot, using market: {}", ticker);
                }
            }
        }

        match client.get_market_prices(&ticker).await {
            Ok(Some(prices)) => {
                let line = format!("[{}] {}", ticker, format_prices_line(&ticker, &prices));
                info!("{}", line);
                append_monitor_log(&line, &now);
            }
            Ok(None) => {
                error!("Failed to fetch prices for {}", ticker);
            }
            Err(e) => {
                error!("Monitor error: {}", e);
            }
        }

        tokio::select! {
            _ = cancel => {
                info!("Stopping monitor...");
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(interval_ms)) => {}
        }
    }

    Ok(())
}
