use super::*;
use crate::{Error, Result};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_retry_executor_with_exponential_backoff() {
    let config = RecoveryConfig {
        max_retries: 3,
        initial_backoff_ms: 10,
        max_backoff_ms: 1000,
        backoff_multiplier: 2.0,
        jitter_factor: 0.0, // No jitter for predictable testing
        enable_circuit_breaker: false,
        ..Default::default()
    };

    let mut executor = RetryExecutor::new(config);
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = Arc::clone(&counter);

    let start_time = std::time::Instant::now();
    
    let result = executor
        .execute(|| {
            let counter = Arc::clone(&counter_clone);
            async move {
                let count = counter.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(Error::Connection("Temporary failure".to_string()))
                } else {
                    Ok("Success")
                }
            }
        })
        .await;

    let elapsed = start_time.elapsed();
    
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Success");
    assert_eq!(counter.load(Ordering::SeqCst), 3); // 2 failures + 1 success
    
    // Should have some delay due to backoff (at least 10ms + 20ms = 30ms)
    assert!(elapsed >= Duration::from_millis(25));
}

#[tokio::test]
async fn test_retry_executor_with_circuit_breaker() {
    let config = RecoveryConfig {
        max_retries: 2,
        initial_backoff_ms: 10,
        enable_circuit_breaker: true,
        circuit_breaker_threshold: 3,
        circuit_breaker_reset_ms: 100,
        ..Default::default()
    };

    let mut executor = RetryExecutor::new(config);
    
    // First few operations should fail and open the circuit
    for _ in 0..3 {
        let result: Result<&str> = executor
            .execute(|| async { Err(Error::Connection("Persistent failure".to_string())) })
            .await;
        assert!(result.is_err());
    }
    
    // Circuit should now be open
    assert_eq!(executor.circuit_breaker_state(), Some(CircuitBreakerState::Open));
    
    // Next operation should fail immediately due to open circuit
    let result = executor
        .execute(|| async { Ok("Should not execute") })
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Circuit breaker is open"));
}

#[tokio::test]
async fn test_circuit_breaker_recovery() {
    let config = RecoveryConfig {
        circuit_breaker_threshold: 2,
        circuit_breaker_reset_ms: 50, // Short timeout for testing
        ..Default::default()
    };

    let mut cb = CircuitBreaker::new(config);
    
    // Trigger circuit breaker
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitBreakerState::Open);
    assert!(!cb.can_execute());
    
    // Wait for reset timeout
    sleep(Duration::from_millis(60)).await;
    
    // Should transition to half-open
    assert!(cb.can_execute());
    assert_eq!(cb.state(), CircuitBreakerState::HalfOpen);
    
    // Multiple successes should close the circuit
    cb.record_success();
    cb.record_success();
    cb.record_success();
    assert_eq!(cb.state(), CircuitBreakerState::Closed);
}

#[tokio::test]
async fn test_graceful_degradation() {
    let config = RecoveryConfig {
        circuit_breaker_threshold: 2,
        ..Default::default()
    };

    let mut degradation = GracefulDegradation::new(config);
    
    // Test feature enabling/disabling
    assert!(degradation.is_feature_enabled("metrics"));
    assert!(degradation.is_feature_enabled("logging"));
    
    // Record failures for metrics feature
    degradation.record_feature_failure("metrics");
    assert!(degradation.is_feature_enabled("metrics"));
    assert_eq!(degradation.feature_failure_count("metrics"), 1);
    
    degradation.record_feature_failure("metrics");
    assert!(!degradation.is_feature_enabled("metrics"));
    assert_eq!(degradation.feature_failure_count("metrics"), 2);
    
    // Logging should still be enabled
    assert!(degradation.is_feature_enabled("logging"));
    
    // Success should re-enable metrics
    degradation.record_feature_success("metrics");
    assert!(degradation.is_feature_enabled("metrics"));
    assert_eq!(degradation.feature_failure_count("metrics"), 0);
    
    // Test disabled features list
    degradation.record_feature_failure("logging");
    degradation.record_feature_failure("logging");
    let disabled = degradation.disabled_features();
    assert!(disabled.contains(&"logging".to_string()));
}

#[tokio::test]
async fn test_retry_with_custom_condition() {
    let config = RecoveryConfig {
        max_retries: 3,
        initial_backoff_ms: 10,
        enable_circuit_breaker: false,
        ..Default::default()
    };

    let mut executor = RetryExecutor::new(config);
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = Arc::clone(&counter);

    // Custom retry condition: only retry on connection errors
    let result = executor
        .execute_with_condition(
            || {
                let counter = Arc::clone(&counter_clone);
                async move {
                    let count = counter.fetch_add(1, Ordering::SeqCst);
                    match count {
                        0 => Err(Error::Connection("Retry this".to_string())),
                        1 => Err(Error::Config("Don't retry this".to_string())),
                        _ => Ok("Success"),
                    }
                }
            },
            |error| matches!(error, Error::Connection(_)),
        )
        .await;

    // Should fail on the config error (second attempt) without retrying
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Don't retry this"));
    assert_eq!(counter.load(Ordering::SeqCst), 2); // 1 connection error + 1 config error
}

#[tokio::test]
async fn test_retry_strategy_delays() {
    // Test exponential backoff
    let strategy = RetryStrategy::ExponentialBackoff {
        initial: Duration::from_millis(100),
        max: Duration::from_millis(1000),
        multiplier: 2.0,
        jitter: 0.0,
    };

    assert_eq!(strategy.delay(0), Duration::from_millis(100));
    assert_eq!(strategy.delay(1), Duration::from_millis(200));
    assert_eq!(strategy.delay(2), Duration::from_millis(400));
    assert_eq!(strategy.delay(10), Duration::from_millis(1000)); // Capped at max

    // Test linear backoff
    let strategy = RetryStrategy::Linear {
        initial: Duration::from_millis(100),
        increment: Duration::from_millis(50),
        max: Duration::from_millis(500),
    };

    assert_eq!(strategy.delay(0), Duration::from_millis(100));
    assert_eq!(strategy.delay(1), Duration::from_millis(150));
    assert_eq!(strategy.delay(2), Duration::from_millis(200));
    assert_eq!(strategy.delay(10), Duration::from_millis(500)); // Capped at max

    // Test fixed delay
    let strategy = RetryStrategy::Fixed(Duration::from_millis(200));
    assert_eq!(strategy.delay(0), Duration::from_millis(200));
    assert_eq!(strategy.delay(5), Duration::from_millis(200));
}

#[tokio::test]
async fn test_timeout_handling() {
    let config = RecoveryConfig {
        max_retries: 2,
        initial_backoff_ms: 10,
        retry_timeout_ms: 50, // Short timeout
        enable_circuit_breaker: false,
        ..Default::default()
    };

    let mut executor = RetryExecutor::new(config);
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = Arc::clone(&counter);

    let result = executor
        .execute(|| {
            let counter = Arc::clone(&counter_clone);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                // Sleep longer than timeout
                sleep(Duration::from_millis(100)).await;
                Ok("Should timeout")
            }
        })
        .await;

    // Should fail due to timeout
    assert!(result.is_err());
    // Should have attempted multiple times due to retries
    assert!(counter.load(Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn test_database_recovery_integration() {
    let config = RecoveryConfig {
        max_retries: 2,
        initial_backoff_ms: 10,
        enable_circuit_breaker: true,
        circuit_breaker_threshold: 2,
        ..Default::default()
    };

    let mut db_recovery = DatabaseRecovery::new(config);
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = Arc::clone(&counter);

    // Test successful operation after retries
    let result = db_recovery
        .execute_db_operation(|| {
            let counter = Arc::clone(&counter_clone);
            async move {
                let count = counter.fetch_add(1, Ordering::SeqCst);
                if count < 1 {
                    Err(Error::Database(sqlx::Error::PoolClosed))
                } else {
                    Ok("Database operation successful")
                }
            }
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Database operation successful");
    assert!(db_recovery.is_database_available());
}

#[tokio::test]
async fn test_recovery_config_validation() {
    let config = RecoveryConfig::default();
    
    // Test default values
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.initial_backoff_ms, 1000);
    assert_eq!(config.max_backoff_ms, 30000);
    assert_eq!(config.backoff_multiplier, 2.0);
    assert_eq!(config.jitter_factor, 0.1);
    assert!(config.enable_circuit_breaker);
    assert_eq!(config.circuit_breaker_threshold, 5);
    
    // Test that config can be serialized/deserialized
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: RecoveryConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config.max_retries, deserialized.max_retries);
}

#[tokio::test]
async fn test_concurrent_circuit_breaker_access() {
    let config = RecoveryConfig {
        circuit_breaker_threshold: 3,
        ..Default::default()
    };

    let mut executor = RetryExecutor::new(config);
    let executor = Arc::new(tokio::sync::Mutex::new(executor));
    
    // Spawn multiple concurrent operations
    let mut handles = Vec::new();
    for i in 0..5 {
        let executor_clone = Arc::clone(&executor);
        let handle = tokio::spawn(async move {
            let mut exec = executor_clone.lock().await;
            exec.execute(|| async move {
                if i < 3 {
                    Err(Error::Connection("Concurrent failure".to_string()))
                } else {
                    Ok(format!("Success {}", i))
                }
            }).await
        });
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    let results: Vec<_> = futures::future::join_all(handles).await;
    
    // Check that operations completed (some may succeed, some may fail due to circuit breaker)
    let completed = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(completed, 5); // All operations should complete
}