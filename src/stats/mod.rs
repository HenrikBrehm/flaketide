//! Bayesian flake-probability engine.
//!
//! Beta-Binomial model with uniform prior `Beta(1, 1)`:
//! after observing `f` failures in `n` runs, the posterior is
//! `Beta(1 + f, 1 + n - f)`. We report posterior mean and the
//! equal-tailed 95% credible interval via `statrs::distribution::Beta`'s
//! `inverse_cdf`.

use chrono::Utc;
use statrs::distribution::{Beta, ContinuousCDF};

use crate::domain::{FlakeReport, Thresholds};
use crate::error::{FlaketideError, Result};
use crate::store::Store;

#[derive(Copy, Clone, Debug)]
pub struct PosteriorStats {
    pub mean: f64,
    pub low: f64,
    pub high: f64,
}

/// Posterior summary of `f` failures in `n` runs.
pub fn flake_posterior(failures: u32, runs: u32) -> PosteriorStats {
    let f = failures as f64;
    let n = runs as f64;
    let a = 1.0 + f;
    let b = 1.0 + (n - f);
    let mean = a / (a + b);
    let dist = Beta::new(a, b).expect("a,b > 0");
    let low = dist.inverse_cdf(0.025);
    let high = dist.inverse_cdf(0.975);
    PosteriorStats { mean, low, high }
}

/// Severity drives sort order + CI alerting:
///   severity = mean * confidence * recency
///   confidence = 1 - min(1, (high - low) / hdi_width_max)
///   recency    = exp(-age_days / 14)
pub fn severity(post: &PosteriorStats, age_days: f64, hdi_width_max: f64) -> f64 {
    let width = (post.high - post.low).max(0.0);
    let confidence = 1.0 - (width / hdi_width_max.max(1e-9)).min(1.0);
    let recency = (-age_days.max(0.0) / 14.0).exp();
    (post.mean * confidence * recency).clamp(0.0, 1.0)
}

/// Apply thresholds to decide whether a test counts as "flaky".
pub fn is_flaky(post: &PosteriorStats, failures: u32, runs: u32, thr: &Thresholds) -> bool {
    failures > 0 && failures < runs
        && post.mean >= thr.flake_prob_min
        && (post.high - post.low) <= thr.hdi_width_max
}

/// Recompute flake verdicts from the store, returning them sorted by severity desc.
pub async fn summarize(store: &Store, thr: &Thresholds) -> Result<Vec<FlakeReport>> {
    let obs = store.observations().await?;
    let now = Utc::now();
    let mut out = Vec::new();
    for o in obs {
        let post = flake_posterior(o.failures, o.runs);
        if !is_flaky(&post, o.failures, o.runs, thr) {
            continue;
        }
        let age_days = (now - o.last_seen).num_seconds() as f64 / 86400.0;
        let sev = severity(&post, age_days, thr.hdi_width_max);
        if sev < thr.flake_prob_min {
            continue;
        }
        let messages = store.recent_failure_messages(&o.id, 5).await?;
        out.push(FlakeReport::new(
            o.id,
            o.runs,
            o.failures,
            post.mean,
            post.low,
            post.high,
            sev,
            o.first_seen,
            o.last_seen,
            messages,
        )?);
    }
    out.sort_by(|a, b| b.severity.partial_cmp(&a.severity).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

/// Helper for snapshot/property tests.
pub fn _construct_unchecked(
    failures: u32, runs: u32,
) -> std::result::Result<PosteriorStats, FlaketideError> {
    if failures > runs {
        return Err(FlaketideError::Invariant("failures > runs".into()));
    }
    Ok(flake_posterior(failures, runs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn mean_close_to_observed_for_large_n() {
        let p = flake_posterior(50, 100);
        assert!((p.mean - 0.5).abs() < 0.05);
        assert!(p.high - p.low < 0.25);
    }

    #[test]
    fn never_fails_zero_runs() {
        let p = flake_posterior(0, 0);
        // Uniform prior: mean 0.5, very wide.
        assert!((p.mean - 0.5).abs() < 1e-9);
        assert!(p.low < 0.05 && p.high > 0.95);
    }

    #[test]
    fn severity_drops_with_age() {
        let p = flake_posterior(5, 10);
        let s_fresh = severity(&p, 0.0, 0.3);
        let s_old = severity(&p, 60.0, 0.3);
        assert!(s_fresh > s_old);
    }

    #[test]
    fn confidence_zero_for_wide_interval() {
        let p = PosteriorStats { mean: 0.5, low: 0.0, high: 1.0 };
        assert!(severity(&p, 0.0, 0.3) < 1e-9);
    }

    proptest! {
        #[test]
        fn posterior_bounded(failures in 0u32..1000, n in 0u32..1000) {
            let runs = n.max(failures);
            let p = flake_posterior(failures, runs);
            prop_assert!(p.mean >= 0.0 && p.mean <= 1.0);
            prop_assert!(p.low <= p.mean + 1e-6);
            prop_assert!(p.high >= p.mean - 1e-6);
            prop_assert!(p.low <= p.high + 1e-6);
        }

        #[test]
        fn flake_classification_excludes_extremes(n in 1u32..200) {
            let thr = Thresholds::default();
            let all_pass = flake_posterior(0, n);
            let all_fail = flake_posterior(n, n);
            prop_assert!(!is_flaky(&all_pass, 0, n, &thr));
            prop_assert!(!is_flaky(&all_fail, n, n, &thr));
        }
    }
}
