//! Budget management for LLM call tracking and graceful degradation.

use crate::CoreError;

/// P34: Budget phase for graceful degradation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BudgetPhase {
    /// Under 80% — normal operation.
    Normal,
    /// 80-95% — skip remaining repair iterations, accept current module versions.
    SkipRepairs,
    /// 95-100% — skip remaining modules, go straight to assembly.
    AssembleNow,
    /// Over 100% — save best output and exit gracefully (not a hard error).
    SaveAndExit,
}

/// P34: Determine current budget phase based on LLM call usage.
pub fn budget_phase(
    max_llm_calls: Option<u32>,
    current_llm_calls: u32,
) -> BudgetPhase {
    let max_calls = match max_llm_calls {
        Some(m) => m,
        None => return BudgetPhase::Normal,
    };
    if max_calls == 0 {
        return BudgetPhase::Normal;
    }
    let pct = (current_llm_calls * 100) / max_calls;
    if pct >= 100 {
        BudgetPhase::SaveAndExit
    } else if pct >= 95 {
        BudgetPhase::AssembleNow
    } else if pct >= 80 {
        BudgetPhase::SkipRepairs
    } else {
        BudgetPhase::Normal
    }
}

/// Compute adaptive LLM call budget based on module count.
/// Formula: modules * 10 + 25 (translate + repairs + re-translates + P33 contract + assembly repair).
/// If user specified a limit, use max(adaptive, user_limit).
pub fn compute_adaptive_budget(module_count: usize, user_limit: Option<u32>) -> u32 {
    let adaptive = (module_count as u32) * 10 + 25;
    match user_limit {
        Some(limit) => adaptive.max(limit),
        None => adaptive,
    }
}

/// Check whether the accumulated token usage exceeds the configured budget.
///
/// Returns `Ok(())` if within budget or no budget is set, otherwise
/// returns `CoreError::BudgetExceeded`.
/// P34: LLM call limit is now a soft check — use `budget_phase()` for graceful degradation.
pub(crate) fn check_budget(
    max_tokens_budget: Option<u64>,
    max_llm_calls: Option<u32>,
    metrics: &noricum_ir::MigrationMetrics,
) -> Result<(), CoreError> {
    if let Some(budget) = max_tokens_budget {
        let used = metrics.input_tokens + metrics.output_tokens;
        if used > budget {
            return Err(CoreError::BudgetExceeded { used, budget });
        }
    }
    // P34: LLM call limit is now soft — only hard-fail at 120% to prevent runaway
    if let Some(max_calls) = max_llm_calls
        && metrics.llm_calls > max_calls + max_calls / 5
    {
        return Err(CoreError::Orchestration(format!(
            "LLM call hard limit exceeded: {} calls (max {} + 20% grace)",
            metrics.llm_calls, max_calls
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_llm_budget() {
        assert_eq!(compute_adaptive_budget(10, Some(50)), 125);
        assert_eq!(compute_adaptive_budget(2, Some(50)), 50);
        assert_eq!(compute_adaptive_budget(10, None), 125);
        assert_eq!(compute_adaptive_budget(1, None), 35);
    }

    #[test]
    fn test_budget_phase_normal() {
        assert_eq!(budget_phase(Some(100), 50), BudgetPhase::Normal);
    }

    #[test]
    fn test_budget_phase_skip_repairs() {
        assert_eq!(budget_phase(Some(100), 82), BudgetPhase::SkipRepairs);
    }

    #[test]
    fn test_budget_phase_assemble_now() {
        assert_eq!(budget_phase(Some(100), 96), BudgetPhase::AssembleNow);
    }

    #[test]
    fn test_budget_phase_save_and_exit() {
        assert_eq!(budget_phase(Some(100), 101), BudgetPhase::SaveAndExit);
    }

    #[test]
    fn test_budget_phase_no_limit() {
        assert_eq!(budget_phase(None, 999), BudgetPhase::Normal);
    }
}
