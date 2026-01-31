use anyhow::Result;
use polymarket_kalshi_arbitrage_bot::{
    bot::{MarketFilters, ShortTermArbitrageBot},
    clients::{KalshiClient, PolymarketClient},
    event::MarketPrices,
    gabagool_executor::GabagoolExecutor,
    position_tracker::PositionTracker,
    settlement_checker::SettlementChecker,
    trade_executor::TradeExecutor,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn, Level};

#[tokio::main]
async fn main() -> Result<()> {

    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting Polymarket-Kalshi Arbitrage Bot");

    dotenv::dotenv().ok();

    let polygon_rpc = std::env::var("POLYGON_RPC_URL")
        .unwrap_or_else(|_| "https://polygon-rpc.com".to_string());
    let wallet_key = std::env::var("POLYMARKET_WALLET_PRIVATE_KEY")
        .ok();
    
    let mut polymarket_client = PolymarketClient::new()
        .with_rpc(polygon_rpc);
    
    if let Some(key) = wallet_key {
        polymarket_client = polymarket_client.with_wallet(key);
    } else {
        warn!("⚠️ POLYMARKET_WALLET_PRIVATE_KEY not set - trading will fail!");
    }

    let kalshi_api_key = std::env::var("KALSHI_API_KEY")
        .unwrap_or_else(|_| {
            warn!("⚠️ KALSHI_API_KEY not set - Kalshi API calls will fail!");
            "".to_string()
        });
    let kalshi_api_secret = std::env::var("KALSHI_API_SECRET")
        .unwrap_or_else(|_| {
            warn!("⚠️ KALSHI_API_SECRET not set - Kalshi API calls will fail!");
            "".to_string()
        });
    
    if kalshi_api_key.is_empty() || kalshi_api_secret.is_empty() {
        error!("❌ Kalshi API credentials missing! Set KALSHI_API_KEY and KALSHI_API_SECRET");
        return Err(anyhow::anyhow!("Missing Kalshi API credentials"));
    }
    
    let kalshi_client = KalshiClient::new(kalshi_api_key, kalshi_api_secret);

    let polymarket_client = Arc::new(polymarket_client);
    let kalshi_client = Arc::new(kalshi_client);

    let position_tracker = Arc::new(Mutex::new(PositionTracker::new()));

    let trade_executor = Arc::new(
        TradeExecutor::new(
            (*polymarket_client.clone()).clone(),
            (*kalshi_client.clone()).clone(),
        )
        .with_position_tracker(position_tracker.clone()),
    );

    let gabagool_executor = Arc::new(
        GabagoolExecutor::new(polymarket_client.clone())
            .with_position_tracker(position_tracker.clone()),
    );

    let settlement_checker = Arc::new(SettlementChecker::new(
        polymarket_client.clone(),
        kalshi_client.clone(),
        position_tracker.clone(),
    ));

    let filters = MarketFilters {
        categories: vec!["crypto".to_string()],
        max_hours_until_resolution: 1,
        min_liquidity: 200.0,
    };

    let bot = ShortTermArbitrageBot::new(
        filters,
        0.80,
        0.02,
    );

    let fetch_prices = {
        let pm = polymarket_client.clone();
        let kalshi = kalshi_client.clone();
        move |event_id: &str, platform: &str| {
            let event_id = event_id.to_string();
            let platform = platform.to_string();
            let pm = pm.clone();
            let kalshi = kalshi.clone();
            async move {
                match platform.as_str() {
                    "polymarket" => pm.fetch_prices(&event_id).await.unwrap_or_default(),
                    "kalshi" => kalshi.fetch_prices(&event_id).await.unwrap_or_default(),
                    _ => MarketPrices::new(0.0, 0.0, 0.0),
                }
            }
        }
    };

    info!("Starting dual-strategy scanning (interval: 60s)");
    info!("🎯 Target: Crypto price prediction 15-minute markets ONLY");
    info!("  Strategy 1: Cross-platform arbitrage (Polymarket ↔ Kalshi)");
    info!("  Strategy 2: Gabagool hedged arbitrage (Polymarket only)");
    info!("  Timeframe: 10-30 minutes until resolution");
    info!("  Requirements: Crypto + Price Prediction + 15-minute timeframe");
    info!("Settlement checking (every 5 minutes)");
    
    let mut scan_interval = tokio::time::interval(Duration::from_secs(60));
    let mut settlement_interval = tokio::time::interval(Duration::from_secs(300));

    let fetch_prices_cross = {
        let pm = polymarket_client.clone();
        let kalshi = kalshi_client.clone();
        move |event_id: &str, platform: &str| {
            let event_id = event_id.to_string();
            let platform = platform.to_string();
            let pm = pm.clone();
            let kalshi = kalshi.clone();
            async move {
                match platform.as_str() {
                    "polymarket" => pm.fetch_prices(&event_id).await.unwrap_or_default(),
                    "kalshi" => kalshi.fetch_prices(&event_id).await.unwrap_or_default(),
                    _ => MarketPrices::new(0.0, 0.0, 0.0),
                }
            }
        }
    };

    let fetch_prices_gabagool = {
        let pm = polymarket_client.clone();
        move |event_id: &str| {
            let event_id = event_id.to_string();
            let pm = pm.clone();
            async move {
                pm.fetch_prices(&event_id).await.unwrap_or_default()
            }
        }
    };

    let get_position_balance = {
        let executor = gabagool_executor.clone();
        move |event_id: &str| {
            let event_id = event_id.to_string();
            let executor = executor.clone();
            async move {
                executor.get_position_balance(&event_id).await
            }
        }
    };
    
    loop {
        tokio::select! {
            _ = scan_interval.tick() => {

        let (pm_events, kalshi_events) = tokio::join!(
            polymarket_client.fetch_events(),
            kalshi_client.fetch_events()
        );
        
        let pm_events = pm_events.unwrap_or_default();
        let kalshi_events = kalshi_events.unwrap_or_default();

        let (cross_platform_opps, gabagool_opps) = tokio::join!(

            bot.scan_for_opportunities(&pm_events, &kalshi_events, fetch_prices_cross.clone()),

            bot.scan_gabagool_opportunities(&pm_events, fetch_prices_gabagool.clone(), get_position_balance.clone())
        );

        if !cross_platform_opps.is_empty() {
            info!("🔀 Strategy 1: Found {} cross-platform arbitrage opportunities", cross_platform_opps.len());
            
            let trade_futures: Vec<_> = cross_platform_opps
                .into_iter()
                .map(|(pm_event, kalshi_event, opp)| {
                    let executor = trade_executor.clone();
                    let trade_amount = 100.0;
                    async move {
                        info!(
                            "🚨 Cross-Platform Opportunity: {} - Profit: ${:.4}, ROI: {:.2}%",
                            pm_event.title,
                            opp.net_profit,
                            opp.roi_percent
                        );
                        executor
                            .execute_arbitrage(&opp, &pm_event, &kalshi_event, trade_amount)
                            .await
                    }
                })
                .collect();

            let trade_results = futures::future::join_all(trade_futures).await;

            for result in trade_results {
                match result {
                    Ok(trade_result) => {
                        if trade_result.success {
                            info!(
                                "✅ Cross-platform trade executed! PM: {:?}, Kalshi: {:?}",
                                trade_result.polymarket_order_id, trade_result.kalshi_order_id
                            );
                        } else {
                            warn!(
                                "⚠️ Cross-platform trade failed: {}",
                                trade_result.error.unwrap_or_default()
                            );
                        }
                    }
                    Err(e) => {
                        error!("Error executing cross-platform trade: {}", e);
                    }
                }
            }
        }

        if !gabagool_opps.is_empty() {
            info!("🎯 Strategy 2: Found {} Gabagool opportunities", gabagool_opps.len());
            
            let gabagool_futures: Vec<_> = gabagool_opps
                .into_iter()
                .map(|opp| {
                    let executor = gabagool_executor.clone();
                    let trade_amount = 100.0;
                    async move {
                        info!(
                            "🎯 Gabagool Opportunity: {} - Buy {} @ ${:.4}, Profit: ${:.4} ({:.2}% ROI), Pair Cost: ${:.4}",
                            opp.event.title,
                            opp.cheap_side,
                            opp.cheap_price,
                            opp.net_profit,
                            opp.roi_percent,
                            opp.pair_cost_after
                        );

                        if opp.profit_locked {
                            info!("🔒 Profit already LOCKED for this position!");
                        }

                        executor.execute_trade(&opp, trade_amount).await
                    }
                })
                .collect();

            let gabagool_results = futures::future::join_all(gabagool_futures).await;

            for result in gabagool_results {
                match result {
                    Ok(success) => {
                        if success {
                            info!("✅ Gabagool trade executed successfully!");
                        } else {
                            warn!("⚠️ Gabagool trade execution returned false");
                        }
                    }
                    Err(e) => {
                        error!("Error executing Gabagool trade: {}", e);
                    }
                }
            }
        }

        if !cross_platform_opps.is_empty() || !gabagool_opps.is_empty() {
            let gabagool_stats = gabagool_executor.get_statistics().await;
            info!(
                "📊 Gabagool Stats - Events: {}, YES: {:.2}, NO: {:.2}, Total Cost: ${:.2}, Locked Profit: ${:.2} ({:.2} pairs)",
                gabagool_stats.total_events,
                gabagool_stats.total_yes_qty,
                gabagool_stats.total_no_qty,
                gabagool_stats.total_cost,
                gabagool_stats.locked_profit,
                gabagool_stats.locked_pairs
            );
        }
            }
            _ = settlement_interval.tick() => {

                info!("Checking for settled positions...");
                match settlement_checker.check_settlements().await {
                    Ok(count) => {
                        if count > 0 {
                            info!("✅ {} positions settled!", count);

                            let stats = settlement_checker.get_statistics().await;
                            info!(
                                "📊 Statistics - Total: {}, Open: {}, Won: {}, Lost: {}, Total Profit: ${:.2}",
                                stats.total_positions,
                                stats.open_positions,
                                stats.won_positions,
                                stats.lost_positions,
                                stats.total_profit
                            );

                            if let Ok((pm_balance, kalshi_balance)) = settlement_checker.check_balances().await {
                                info!(
                                    "💰 Current Balances - Polymarket: ${:.2}, Kalshi: ${:.2}, Total: ${:.2}",
                                    pm_balance,
                                    kalshi_balance,
                                    pm_balance + kalshi_balance
                                );
                            }
                        } else {
                            info!("No new settlements");
                        }
                    }
                    Err(e) => {
                        error!("Error checking settlements: {}", e);
                    }
                }
            }
        }
    }
}
