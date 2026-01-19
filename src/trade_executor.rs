use crate::arbitrage_detector::ArbitrageOpportunity;
use crate::clients::{KalshiClient, PolymarketClient};
use crate::event::Event;
use crate::position_tracker::{Position, PositionTracker};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub struct TradeResult {
    pub success: bool,
    pub polymarket_order_id: Option<String>,
    pub kalshi_order_id: Option<String>,
    pub error: Option<String>,
}

pub struct TradeExecutor {
    polymarket_client: PolymarketClient,
    kalshi_client: KalshiClient,
    position_tracker: Option<Arc<Mutex<PositionTracker>>>,
}

impl TradeExecutor {
    pub fn new(polymarket_client: PolymarketClient, kalshi_client: KalshiClient) -> Self {
        Self {
            polymarket_client,
            kalshi_client,
            position_tracker: None,
        }
    }

    pub fn with_position_tracker(mut self, tracker: Arc<Mutex<PositionTracker>>) -> Self {
        self.position_tracker = Some(tracker);
        self
    }

    pub async fn execute_arbitrage(
        &self,
        opportunity: &ArbitrageOpportunity,
        pm_event: &Event,
        kalshi_event: &Event,
        amount: f64,
    ) -> Result<TradeResult> {
        info!(
            "Executing arbitrage: {} - Expected profit: ${:.4} ({:.2}% ROI)",
            opportunity.strategy, opportunity.net_profit, opportunity.roi_percent
        );

        let (pm_result, kalshi_result) = tokio::join!(
            self.execute_polymarket_trade(
                pm_event,
                &opportunity.polymarket_action,
                amount
            ),
            self.execute_kalshi_trade(
                kalshi_event,
                &opportunity.kalshi_action,
                amount
            )
        );

        let pm_success = pm_result.is_ok();
        let kalshi_success = kalshi_result.is_ok();

        if pm_success && kalshi_success {
            info!(
                "✅ Arbitrage executed successfully! PM: {:?}, Kalshi: {:?}",
                pm_result.as_ref().unwrap(),
                kalshi_result.as_ref().unwrap()
            );

            let pm_order_id = pm_result.unwrap();
            let kalshi_order_id = kalshi_result.unwrap();

            if let Some(tracker) = &self.position_tracker {
                let mut tracker = tracker.lock().await;

                let pm_position = Position::new(
                    "polymarket".to_string(),
                    pm_event,
                    opportunity.polymarket_action.1.clone(),
                    amount / opportunity.polymarket_action.2,
                    amount * opportunity.polymarket_action.2,
                    opportunity.polymarket_action.2,
                    pm_order_id.clone(),
                );
                tracker.add_position(pm_position);

                let kalshi_position = Position::new(
                    "kalshi".to_string(),
                    kalshi_event,
                    opportunity.kalshi_action.1.clone(),
                    amount / opportunity.kalshi_action.2,
                    amount * opportunity.kalshi_action.2,
                    opportunity.kalshi_action.2,
                    kalshi_order_id.clone(),
                );
                tracker.add_position(kalshi_position);
            }

            Ok(TradeResult {
                success: true,
                polymarket_order_id: pm_order_id,
                kalshi_order_id: kalshi_order_id,
                error: None,
            })
        } else {

            let mut errors = Vec::new();
            if let Err(e) = pm_result {
                errors.push(format!("Polymarket: {}", e));
            }
            if let Err(e) = kalshi_result {
                errors.push(format!("Kalshi: {}", e));
            }

            let error_msg = errors.join("; ");

            warn!("⚠️ Arbitrage execution failed: {}", error_msg);

            if pm_success {
                warn!("Polymarket trade succeeded but Kalshi failed - may need to cancel PM trade");
            }
            if kalshi_success {
                warn!("Kalshi trade succeeded but Polymarket failed - may need to cancel Kalshi trade");
            }

            Ok(TradeResult {
                success: false,
                polymarket_order_id: pm_result.ok().flatten(),
                kalshi_order_id: kalshi_result.ok().flatten(),
                error: Some(error_msg),
            })
        }
    }

    async fn execute_polymarket_trade(
        &self,
        event: &Event,
        action: &(String, String, f64),
        amount: f64,
    ) -> Result<Option<String>> {
        let (action_type, outcome, max_price) = action;

        info!(
            "Placing {} order on Polymarket: {} {} @ ${:.4} (amount: ${:.2})",
            action_type, outcome, max_price, amount
        );

        match self
            .polymarket_client
            .place_order(
                event.event_id.clone(),
                outcome.clone(),
                amount,
                *max_price,
            )
            .await
        {
            Ok(order_id) => order_id,
            Err(e) => {
                error!("Polymarket order failed: {}", e);
                return Err(e);
            }
        }
        
        info!("✅ Polymarket order placed: {}", order_id);
        Ok(Some(order_id))
    }

    async fn execute_kalshi_trade(
        &self,
        event: &Event,
        action: &(String, String, f64),
        amount: f64,
    ) -> Result<Option<String>> {
        let (action_type, outcome, price) = action;

        info!(
            "Placing {} order on Kalshi: {} {} @ ${:.4} (amount: ${:.2})",
            action_type, outcome, price, amount
        );

        match self
            .kalshi_client
            .place_order(
                event.event_id.clone(),
                outcome.clone(),
                amount,
                *price,
            )
            .await
        {
            Ok(order_id) => order_id,
            Err(e) => {
                error!("Kalshi order failed: {}", e);
                return Err(e);
            }
        }
        
        info!("✅ Kalshi order placed: {}", order_id);
        Ok(Some(order_id))
    }

    pub async fn cancel_order(&self, platform: &str, order_id: &str) -> Result<()> {
        match platform {
            "polymarket" => {

                info!("Cancelling Polymarket order: {}", order_id);
                Ok(())
            }
            "kalshi" => {

                info!("Cancelling Kalshi order: {}", order_id);
                Ok(())
            }
            _ => {
                error!("Unknown platform: {}", platform);
                Err(anyhow::anyhow!("Unknown platform: {}", platform))
            }
        }
    }

    pub async fn get_order_status(&self, platform: &str, order_id: &str) -> Result<String> {
        match platform {
            "polymarket" => {

                Ok("filled".to_string())
            }
            "kalshi" => {

                Ok("filled".to_string())
            }
            _ => Err(anyhow::anyhow!("Unknown platform: {}", platform)),
        }
    }
}

