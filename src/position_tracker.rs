use crate::event::Event;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PositionStatus {
    Open,
    Settled,
    Won,
    Lost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: String,
    pub platform: String,
    pub event_id: String,
    pub event_title: String,
    pub outcome: String,
    pub amount: f64,
    pub cost: f64,
    pub price: f64,
    pub order_id: Option<String>,
    pub status: PositionStatus,
    pub created_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
    pub payout: Option<f64>,
    pub profit: Option<f64>,
}

impl Position {
    pub fn new(
        platform: String,
        event: &Event,
        outcome: String,
        amount: f64,
        cost: f64,
        price: f64,
        order_id: Option<String>,
    ) -> Self {
        Self {
            id: format!("{}_{}", platform, &uuid::Uuid::new_v4().to_string()[..8]),
            platform,
            event_id: event.event_id.clone(),
            event_title: event.title.clone(),
            outcome,
            amount,
            cost,
            price,
            order_id,
            status: PositionStatus::Open,
            created_at: Utc::now(),
            settled_at: None,
            payout: None,
            profit: None,
        }
    }

    pub fn calculate_profit_if_won(&self) -> f64 {

        let payout = self.amount * 1.0;
        payout - self.cost
    }

    pub fn calculate_profit_if_lost(&self) -> f64 {

        -self.cost
    }
}

pub struct PositionTracker {
    positions: HashMap<String, Position>,
}

impl PositionTracker {
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
        }
    }

    pub fn add_position(&mut self, position: Position) {
        info!("📝 Tracking new position: {} - {} {} @ ${:.4}", 
            position.event_title, 
            position.outcome,
            position.amount,
            position.price
        );
        self.positions.insert(position.id.clone(), position);
    }

    pub fn get_open_positions(&self) -> Vec<&Position> {
        self.positions
            .values()
            .filter(|p| p.status == PositionStatus::Open)
            .collect()
    }

    pub fn get_all_positions(&self) -> Vec<&Position> {
        self.positions.values().collect()
    }

    pub fn get_positions_by_platform(&self, platform: &str) -> Vec<&Position> {
        self.positions
            .values()
            .filter(|p| p.platform == platform)
            .collect()
    }

    pub fn update_position_settlement(
        &mut self,
        position_id: &str,
        won: bool,
        payout: Option<f64>,
    ) -> Option<f64> {
        if let Some(position) = self.positions.get_mut(position_id) {
            position.status = if won {
                PositionStatus::Won
            } else {
                PositionStatus::Lost
            };
            position.settled_at = Some(Utc::now());
            position.payout = payout;

            let profit = if won {
                position.calculate_profit_if_won()
            } else {
                position.calculate_profit_if_lost()
            };
            position.profit = Some(profit);

            info!(
                "💰 Position settled: {} - {} - Profit: ${:.2}",
                position.event_title,
                if won { "WON" } else { "LOST" },
                profit
            );

            Some(profit)
        } else {
            None
        }
    }

    pub fn get_total_profit(&self) -> f64 {
        self.positions
            .values()
            .filter_map(|p| p.profit)
            .sum()
    }

    pub fn get_profit_by_platform(&self, platform: &str) -> f64 {
        self.positions
            .values()
            .filter(|p| p.platform == platform)
            .filter_map(|p| p.profit)
            .sum()
    }

    pub fn get_statistics(&self) -> PositionStatistics {
        let total = self.positions.len();
        let open = self.positions.values().filter(|p| p.status == PositionStatus::Open).count();
        let won = self.positions.values().filter(|p| p.status == PositionStatus::Won).count();
        let lost = self.positions.values().filter(|p| p.status == PositionStatus::Lost).count();
        let total_profit = self.get_total_profit();

        PositionStatistics {
            total_positions: total,
            open_positions: open,
            won_positions: won,
            lost_positions: lost,
            total_profit,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PositionStatistics {
    pub total_positions: usize,
    pub open_positions: usize,
    pub won_positions: usize,
    pub lost_positions: usize,
    pub total_profit: f64,
}

