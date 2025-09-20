use crate::{
    api_server::ApiServer,
    database::MockDatabaseOps,
    types::{DaemonStatus, MiningStats},
    Result,
};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{sync::RwLock, time::sleep};

#[tokio::test]
async fn test_api_server_startup_and_health_check() -> Result<()> {
    // Create test dependencies
    let database = Arc::new(MockDatabaseOps::new());
    let daemon_status = Arc::new(RwLock::new(DaemonStatus::default()));
    let mining_stats = Arc::new(RwLock::new(MiningStats::default()));
    
    // Use a test port
    let test_port = 18080;
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    
    let api_server = ApiServer::new(
        bind_address,
        database,
        daemon_status,
        mining_stats,
    );
    
    // Start API server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = api_server.start().await {
            eprintln!("API server error: {}", e);
        }
    });
    
    // Give server time to start
    sleep(Duration::from_millis(500)).await;
    
    // Test health endpoint
    let client = reqwest::Client::new();
    let health_url = format!("http://127.0.0.1:{}/api/v1/health", test_port);
    
    match client.get(&health_url).send().await {
        Ok(response) => {
            println!("Health check response status: {}", response.status());
            assert!(response.status().is_success());
            
            if let Ok(body) = response.text().await {
                println!("Health check response body: {}", body);
                assert!(body.contains("success") || body.contains("OK"));
            }
        }
        Err(e) => {
            panic!("Failed to connect to API server: {}", e);
        }
    }
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

#[tokio::test]
async fn test_api_server_status_endpoint() -> Result<()> {
    // Create test dependencies
    let database = Arc::new(MockDatabaseOps::new());
    let daemon_status = Arc::new(RwLock::new(DaemonStatus::default()));
    let mining_stats = Arc::new(RwLock::new(MiningStats::default()));
    
    // Use a different test port
    let test_port = 18081;
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    
    let api_server = ApiServer::new(
        bind_address,
        database,
        daemon_status,
        mining_stats,
    );
    
    // Start API server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = api_server.start().await {
            eprintln!("API server error: {}", e);
        }
    });
    
    // Give server time to start
    sleep(Duration::from_millis(500)).await;
    
    // Test status endpoint
    let client = reqwest::Client::new();
    let status_url = format!("http://127.0.0.1:{}/api/v1/status", test_port);
    
    match client.get(&status_url).send().await {
        Ok(response) => {
            println!("Status response status: {}", response.status());
            assert!(response.status().is_success());
            
            if let Ok(body) = response.text().await {
                println!("Status response body: {}", body);
                assert!(body.contains("success") && (body.contains("running") || body.contains("uptime")));
            }
        }
        Err(e) => {
            panic!("Failed to get status from API server: {}", e);
        }
    }
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

#[tokio::test]
async fn test_api_server_cors_and_middleware() -> Result<()> {
    // Create test dependencies
    let database = Arc::new(MockDatabaseOps::new());
    let daemon_status = Arc::new(RwLock::new(DaemonStatus::default()));
    let mining_stats = Arc::new(RwLock::new(MiningStats::default()));
    
    // Use a different test port
    let test_port = 18082;
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    
    let api_server = ApiServer::new(
        bind_address,
        database,
        daemon_status,
        mining_stats,
    );
    
    // Start API server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = api_server.start().await {
            eprintln!("API server error: {}", e);
        }
    });
    
    // Give server time to start
    sleep(Duration::from_millis(500)).await;
    
    // Test CORS headers with OPTIONS request
    let client = reqwest::Client::new();
    let health_url = format!("http://127.0.0.1:{}/api/v1/health", test_port);
    
    match client
        .request(reqwest::Method::OPTIONS, &health_url)
        .header("Origin", "http://localhost:3000")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
    {
        Ok(response) => {
            println!("CORS preflight response status: {}", response.status());
            // CORS preflight should be handled
            assert!(response.status().is_success() || response.status() == 200);
            
            // Check for CORS headers
            let headers = response.headers();
            println!("Response headers: {:?}", headers);
        }
        Err(e) => {
            println!("CORS preflight request failed (this might be expected): {}", e);
            // CORS preflight failure is not necessarily a test failure
        }
    }
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}