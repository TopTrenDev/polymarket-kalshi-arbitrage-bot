use crate::event::{Event, MarketPrices};

#[derive(Debug, Clone)]
pub struct GabagoolOpportunity {
    pub event: Event,
    pub cheap_side: String,
    pub cheap_price: f64,
    pub net_profit: f64,
    pub roi_percent: f64,
    pub pair_cost_after: f64,
    pub total_cost: f64,
    pub profit_locked: bool,
}

pub struct GabagoolDetector {
    min_profit_threshold: f64,
}

impl GabagoolDetector {
    pub fn new(min_profit_threshold: f64) -> Self {
        Self {
            min_profit_threshold,
        }
    }

    pub fn check_opportunity(
        &self,
        event: &Event,
        prices: &MarketPrices,
        yes_qty: f64,
        no_qty: f64,
        yes_cost: f64,
        no_cost: f64,
    ) -> Option<GabagoolOpportunity> {
        let yes_ask = prices.yes_ask_or_fallback();
        let no_ask = prices.no_ask_or_fallback();

        if yes_ask <= 0.0 || no_ask <= 0.0 {
            return None;
        }

        let min_pairs = yes_qty.min(no_qty);
        let profit_locked = min_pairs > 0.0 && (yes_cost + no_cost) / min_pairs < 1.0;

        let (cheap_side, cheap_price) = if yes_ask <= no_ask {
            ("YES".to_string(), yes_ask)
        } else {
            ("NO".to_string(), no_ask)
        };

        let target_side = if (yes_qty - no_qty).abs() > 0.01 {
            if yes_qty < no_qty {
                "YES".to_string()
            } else {
                "NO".to_string()
            }
        } else {
            cheap_side.clone()
        };

        let buy_price = if target_side == "YES" { yes_ask } else { no_ask };
        let unit_cost = buy_price;

        let (new_yes_qty, new_no_qty, new_yes_cost, new_no_cost) = if target_side == "YES" {
            (yes_qty + 1.0, no_qty, yes_cost + unit_cost, no_cost)
        } else {
            (yes_qty, no_qty + 1.0, yes_cost, no_cost + unit_cost)
        };

        let new_min_pairs = new_yes_qty.min(new_no_qty);
        if new_min_pairs <= 0.0 {
            return None;
        }

        let pair_cost_after = (new_yes_cost + new_no_cost) / new_min_pairs;
        if pair_cost_after >= 1.0 {
            return None;
        }

        let net_profit = 1.0 - pair_cost_after;
        if net_profit <= self.min_profit_threshold && !profit_locked {
            return None;
        }

        let total_cost = pair_cost_after;
        let roi_percent = (net_profit / total_cost) * 100.0;

        Some(GabagoolOpportunity {
            event: event.clone(),
            cheap_side: target_side,
            cheap_price: buy_price,
            net_profit,
            roi_percent,
            pair_cost_after,
            total_cost,
            profit_locked,
        })
    }
}
