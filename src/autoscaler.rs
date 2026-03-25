//! Auto-scaler for selfware based on GPU utilization
//! Dynamically adjusts concurrent instances to maximize throughput

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;

/// GPU utilization metrics
#[derive(Debug, Clone, Copy)]
pub struct GPUMetrics {
    pub gpu_id: u32,
    pub utilization: u32,      // 0-100%
    pub memory_used: u64,      // MB
    pub memory_total: u64,     // MB
    pub temperature: u32,      // Celsius
    pub power_draw: f32,       // Watts
}

/// Auto-scaler configuration
#[derive(Debug, Clone)]
pub struct AutoScalerConfig {
    /// Target GPU utilization (default: 85%)
    pub target_utilization: u32,
    /// Min concurrent instances
    pub min_instances: usize,
    /// Max concurrent instances
    pub max_instances: usize,
    /// Scale up threshold (if above this for N checks)
    pub scale_up_threshold: u32,
    /// Scale down threshold (if below this for N checks)
    pub scale_down_threshold: u32,
    /// Check interval in seconds
    pub check_interval_secs: u64,
    /// Number of checks before scaling
    pub checks_before_scale: usize,
}

impl Default for AutoScalerConfig {
    fn default() -> Self {
        Self {
            target_utilization: 85,
            min_instances: 4,
            max_instances: 32,
            scale_up_threshold: 90,
            scale_down_threshold: 50,
            check_interval_secs: 60,
            checks_before_scale: 3,
        }
    }
}

/// Auto-scaler state
pub struct AutoScaler {
    config: AutoScalerConfig,
    current_instances: Arc<RwLock<usize>>,
    utilization_history: Arc<RwLock<VecDeque<u32>>>,
    last_scale: Arc<RwLock<Instant>>,
    scale_cooldown: Duration,
}

impl AutoScaler {
    pub fn new(config: AutoScalerConfig) -> Self {
        Self {
            config,
            current_instances: Arc::new(RwLock::new(config.min_instances)),
            utilization_history: Arc::new(RwLock::new(VecDeque::with_capacity(10))),
            last_scale: Arc::new(RwLock::new(Instant::now())),
            scale_cooldown: Duration::from_secs(300), // 5 min cooldown
        }
    }

    /// Start the auto-scaler monitoring loop
    pub async fn start(&self) {
        let mut ticker = interval(Duration::from_secs(self.config.check_interval_secs));
        
        loop {
            ticker.tick().await;
            
            if let Err(e) = self.check_and_scale().await {
                eprintln!("Auto-scaler error: {}", e);
            }
        }
    }

    /// Get current recommended instance count
    pub async fn get_instance_count(&self) -> usize {
        *self.current_instances.read().await
    }

    /// Check GPU metrics and scale if needed
    async fn check_and_scale(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Get GPU metrics (simulated - would use nvidia-smi in real impl)
        let metrics = self.get_gpu_metrics().await?;
        
        // Calculate average utilization across all GPUs
        let avg_util: u32 = metrics.iter()
            .map(|m| m.utilization)
            .sum::<u32>() / metrics.len().max(1) as u32;
        
        // Add to history
        {
            let mut history = self.utilization_history.write().await;
            history.push_back(avg_util);
            if history.len() > 10 {
                history.pop_front();
            }
        }
        
        // Check cooldown
        let last_scale = *self.last_scale.read().await;
        if last_scale.elapsed() < self.scale_cooldown {
            return Ok(());
        }
        
        // Check if we should scale
        let history = self.utilization_history.read().await;
        let recent_checks: Vec<u32> = history.iter().rev()
            .take(self.config.checks_before_scale)
            .copied()
            .collect();
        
        if recent_checks.len() < self.config.checks_before_scale {
            return Ok(()); // Not enough data yet
        }
        
        let current = *self.current_instances.read().await;
        
        // Scale up if consistently above threshold
        if recent_checks.iter().all(|&u| u >= self.config.scale_up_threshold) 
            && current < self.config.max_instances {
            let new_count = (current * 2).min(self.config.max_instances);
            self.scale_to(new_count).await;
            println!("🚀 Auto-scaler: {} → {} instances (GPU: {}%)", current, new_count, avg_util);
        }
        // Scale down if consistently below threshold
        else if recent_checks.iter().all(|&u| u <= self.config.scale_down_threshold)
            && current > self.config.min_instances {
            let new_count = (current / 2).max(self.config.min_instances);
            self.scale_to(new_count).await;
            println!("📉 Auto-scaler: {} → {} instances (GPU: {}%)", current, new_count, avg_util);
        }
        
        Ok(())
    }

    /// Scale to specific instance count
    async fn scale_to(&self, count: usize) {
        let mut instances = self.current_instances.write().await;
        *instances = count;
        *self.last_scale.write().await = Instant::now();
    }

    /// Get GPU metrics from nvidia-smi
    async fn get_gpu_metrics(&self) -> Result<Vec<GPUMetrics>, Box<dyn std::error::Error>> {
        // This would actually call nvidia-smi
        // For now, return simulated data
        Ok(vec![
            GPUMetrics {
                gpu_id: 0,
                utilization: 85,
                memory_used: 23000,
                memory_total: 24564,
                temperature: 70,
                power_draw: 300.0,
            },
            GPUMetrics {
                gpu_id: 1,
                utilization: 88,
                memory_used: 22800,
                memory_total: 24564,
                temperature: 68,
                power_draw: 295.0,
            },
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_autoscaler_scaling() {
        let config = AutoScalerConfig {
            target_utilization: 85,
            min_instances: 4,
            max_instances: 32,
            scale_up_threshold: 90,
            scale_down_threshold: 50,
            check_interval_secs: 1,
            checks_before_scale: 2,
        };
        
        let scaler = AutoScaler::new(config);
        
        // Should start at min_instances
        assert_eq!(scaler.get_instance_count().await, 4);
        
        // After scaling up
        scaler.scale_to(8).await;
        assert_eq!(scaler.get_instance_count().await, 8);
    }
}
