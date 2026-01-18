use crate::clients::PolymarketClient;
use crate::event::Event;
use crate::gabagool_detector::GabagoolOpportunity;
use crate::position_tracker::{Position, PositionTracker};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// Tracks Gabagool positions per event
#[derive(Debug, Clone)]
struct GabagoolPosition {
    event_id: String,
    yes_qty: f64,
    yes_cost: f64,
    no_qty: f64,
    no_cost: f64,
}

pub struct GabagoolExecutor {
    polymarket_client: Arc<PolymarketClient>,
    position_tracker: Option<Arc<Mutex<PositionTracker>>>,
    gabagool_positions: Arc<Mutex<HashMap<String, GabagoolPosition>>>,
}

impl GabagoolExecutor {
    pub fn new(polymarket_client: Arc<PolymarketClient>) -> Self {
        Self {
            polymarket_client,
            position_tracker: None,
            gabagool_positions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_position_tracker(mut self, tracker: Arc<Mutex<PositionTracker>>) -> Self {
        self.position_tracker = Some(tracker);
        self
    }

    /// Get current position balance for an event
    pub async fn get_position_balance(&self, event_id: &str) -> (f64, f64, f64, f64) {
        let positions = self.gabagool_positions.lock().await;
        if let Some(pos) = positions.get(event_id) {
            (
                pos.yes_qty,
                pos.yes_cost,
                pos.no_qty,
                pos.no_cost,
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        }
    }

    /// Execute a Gabagool trade (buy the cheap side)
    pub async fn execute_trade(
        &self,
        opportunity: &GabagoolOpportunity,
        amount: f64,
    ) -> Result<bool> {
        info!(
            "🎯 Executing Gabagool trade: {} - Buy {} @ ${:.4} (Total cost: ${:.4}, Profit: ${:.4} ({:.2}% ROI))",
            opportunity.event.title,
            opportunity.cheap_side,
            opportunity.cheap_price,
            opportunity.total_cost,
            opportunity.net_profit,
            opportunity.roi_percent
        );

        // Calculate number of shares to buy
        let shares = amount / opportunity.cheap_price;

        // Place order on Polymarket
        let order_id = self
            .polymarket_client
            .place_order(
                opportunity.event.event_id.clone(),
                opportunity.cheap_side.clone(),
                amount,
                opportunity.cheap_price,
            )
            .await?;

        if order_id.is_none() {
            warn!("⚠️ Gabagool order placed but no order ID returned");
        }

        // Update position balance
        let mut positions = self.gabagool_positions.lock().await;
        let position = positions
            .entry(opportunity.event.event_id.clone())
            .or_insert_with(|| GabagoolPosition {
                event_id: opportunity.event.event_id.clone(),
                yes_qty: 0.0,
                yes_cost: 0.0,
                no_qty: 0.0,
                no_cost: 0.0,
            });

        if opportunity.cheap_side == "YES" {
            position.yes_qty += shares;
            position.yes_cost += amount;
        } else {
            position.no_qty += shares;
            position.no_cost += amount;
        }

        let new_yes_qty = position.yes_qty;
        let new_no_qty = position.no_qty;
        let new_yes_cost = position.yes_cost;
        let new_no_cost = position.no_cost;

        drop(positions);

        // Track in main position tracker
        if let Some(tracker) = &self.position_tracker {
            let mut tracker = tracker.lock().await;
            let position = Position::new(
                "polymarket".to_string(),
                &opportunity.event,
                opportunity.cheap_side.clone(),
                shares,
                amount,
                opportunity.cheap_price,
                order_id,
            );
            tracker.add_position(position);
        }

        // Log position status
        let min_qty = new_yes_qty.min(new_no_qty);
        let pair_cost = if min_qty > 0.0 {
            (new_yes_cost + new_no_cost) / min_qty
        } else {
            opportunity.total_cost
        };

        info!(
            "📊 Position updated - YES: {:.2} (${:.2}), NO: {:.2} (${:.2}), Pairs: {:.2}, Pair Cost: ${:.4}",
            new_yes_qty, new_yes_cost, new_no_qty, new_no_cost, min_qty, pair_cost
        );

        if pair_cost < 1.0 && min_qty > 0.0 {
            let locked_profit = (1.0 - pair_cost) * min_qty;
            info!(
                "🔒 Profit LOCKED! ${:.2} guaranteed profit on {:.2} pairs",
                locked_profit, min_qty
            );
        }

        Ok(true)
    }

    /// Get statistics for all Gabagool positions
    pub async fn get_statistics(&self) -> GabagoolStatistics {
        let positions = self.gabagool_positions.lock().await;
        
        let mut total_events = 0;
        let mut total_yes_qty = 0.0;
        let mut total_no_qty = 0.0;
        let mut total_yes_cost = 0.0;
        let mut total_no_cost = 0.0;
        let mut locked_profit = 0.0;
        let mut locked_pairs = 0.0;

        for pos in positions.values() {
            total_events += 1;
            total_yes_qty += pos.yes_qty;
            total_no_qty += pos.no_qty;
            total_yes_cost += pos.yes_cost;
            total_no_cost += pos.no_cost;

            let min_qty = pos.yes_qty.min(pos.no_qty);
            if min_qty > 0.0 {
                let pair_cost = (pos.yes_cost + pos.no_cost) / min_qty;
                if pair_cost < 1.0 {
                    locked_pairs += min_qty;
                    locked_profit += (1.0 - pair_cost) * min_qty;
                }
            }
        }

        GabagoolStatistics {
            total_events,
            total_yes_qty,
            total_no_qty,
            total_yes_cost,
            total_no_cost,
            total_cost: total_yes_cost + total_no_cost,
            locked_profit,
            locked_pairs,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GabagoolStatistics {
    pub total_events: usize,
    pub total_yes_qty: f64,
    pub total_no_qty: f64,
    pub total_yes_cost: f64,
    pub total_no_cost: f64,
    pub total_cost: f64,
    pub locked_profit: f64,
    pub locked_pairs: f64,
}

