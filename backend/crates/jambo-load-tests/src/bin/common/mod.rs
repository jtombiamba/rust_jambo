use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PercentileStats {
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub avg: f64,
    pub count: usize,
}

pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

pub fn compute_percentile_stats(values: &[f64]) -> PercentileStats {
    if values.is_empty() {
        return PercentileStats {
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
            avg: 0.0,
            count: 0,
        };
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg = sorted.iter().sum::<f64>() / sorted.len() as f64;
    PercentileStats {
        p50: percentile(&sorted, 50.0),
        p95: percentile(&sorted, 95.0),
        p99: percentile(&sorted, 99.0),
        avg,
        count: values.len(),
    }
}

pub async fn run_cleanup(
    client: &reqwest::Client,
    target_url: &str,
    benchmark_token: &str,
) -> Result<()> {
    let mut req = client.post(format!("{}/api/benchmark/cleanup", target_url));
    if !benchmark_token.is_empty() {
        req = req.header("X-Benchmark-Token", benchmark_token);
    }
    let resp = req.send().await?;

    if resp.status().is_success() {
        let body: serde_json::Value = resp.json().await?;
        tracing::info!("Cleanup complete: {}", serde_json::to_string_pretty(&body)?);
    } else {
        tracing::warn!("Cleanup failed: {}", resp.text().await.unwrap_or_default());
    }
    Ok(())
}
