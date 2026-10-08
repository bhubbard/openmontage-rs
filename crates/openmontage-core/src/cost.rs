use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostRecord {
    pub provider: String,
    pub operation: String,
    pub units: f64,
    pub unit_type: String,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub total_cost_usd: f64,
    pub budget_usd: f64,
    pub remaining_usd: f64,
    pub records: Vec<CostRecord>,
}

#[derive(Debug, Clone)]
pub struct CostTracker {
    budget_usd: f64,
    records: Vec<CostRecord>,
}

impl CostTracker {
    pub fn new(budget_usd: f64) -> Self {
        Self {
            budget_usd,
            records: Vec::new(),
        }
    }

    pub fn record(
        &mut self,
        provider: &str,
        operation: &str,
        units: f64,
        unit_type: &str,
        unit_rate_usd: f64,
    ) {
        let cost_usd = units * unit_rate_usd;
        self.records.push(CostRecord {
            provider: provider.to_string(),
            operation: operation.to_string(),
            units,
            unit_type: unit_type.to_string(),
            cost_usd,
        });
    }

    pub fn total_cost(&self) -> f64 {
        self.records.iter().map(|r| r.cost_usd).sum()
    }

    pub fn remaining_budget(&self) -> f64 {
        self.budget_usd - self.total_cost()
    }

    pub fn is_budget_exceeded(&self) -> bool {
        self.total_cost() > self.budget_usd
    }

    pub fn summary(&self) -> CostSummary {
        let total = self.total_cost();
        CostSummary {
            total_cost_usd: total,
            budget_usd: self.budget_usd,
            remaining_usd: self.budget_usd - total,
            records: self.records.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_tracking() {
        let mut tracker = CostTracker::new(5.0);
        tracker.record("openai", "script_generation", 1500.0, "tokens", 0.00003);
        tracker.record("fal", "video_diffusion", 10.0, "seconds", 0.15);

        let summary = tracker.summary();
        assert!(summary.total_cost_usd > 1.5);
        assert!(!tracker.is_budget_exceeded());
    }
}
