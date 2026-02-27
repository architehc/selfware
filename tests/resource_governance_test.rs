use selfware::config::{ResourcesConfig, ResourceQuotas};
use selfware::resource::{AdaptiveQuotas, ResourcePressure};
use selfware::resource::quotas::ResourceLimitTracker;
use std::sync::Arc;

#[tokio::test]
async fn test_resource_limit_tracker_quotas() {
    let mut config = ResourcesConfig::default();
    config.quotas.max_gpu_memory_per_model = 1000;
    config.quotas.max_concurrent_requests = 2;
    config.quotas.max_queued_tasks = 5;

    let tracker = ResourceLimitTracker::new(config.quotas.clone());

    // Test GPU memory allocation
    let guard1 = tracker.allocate_gpu_memory(500).unwrap();
    let guard2 = tracker.allocate_gpu_memory(500).unwrap();
    
    // This should fail
    let guard3_result = tracker.allocate_gpu_memory(1);
    assert!(guard3_result.is_err(), "Should exceed GPU memory quota");
    
    drop(guard1);
    
    // Now it should succeed
    let _guard3 = tracker.allocate_gpu_memory(1).unwrap();

    // Test concurrent requests
    let req1 = tracker.start_request().unwrap();
    let req2 = tracker.start_request().unwrap();
    
    let req3_result = tracker.start_request();
    assert!(req3_result.is_err(), "Should exceed concurrent requests quota");
    
    drop(req2);
    
    let _req3 = tracker.start_request().unwrap();
}

#[tokio::test]
async fn test_adaptive_quotas() {
    let base_quotas = ResourceQuotas::default();
    let adaptive = AdaptiveQuotas::new(base_quotas.clone());
    
    // Test medium pressure
    adaptive.adjust_for_pressure(ResourcePressure::Medium).await;
    let current = adaptive.current().await;
    assert_eq!(current.max_concurrent_requests, base_quotas.max_concurrent_requests / 2);
    assert_eq!(current.max_context_tokens, base_quotas.max_context_tokens / 2);
    
    // Test critical pressure
    adaptive.adjust_for_pressure(ResourcePressure::Critical).await;
    let current_crit = adaptive.current().await;
    assert_eq!(current_crit.max_concurrent_requests, 1);
    assert_eq!(current_crit.max_context_tokens, 8192);
    
    // Reset
    adaptive.adjust_for_pressure(ResourcePressure::None).await;
    let current_none = adaptive.current().await;
    assert_eq!(current_none.max_concurrent_requests, base_quotas.max_concurrent_requests);
}
