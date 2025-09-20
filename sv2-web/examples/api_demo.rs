use reqwest;
use serde_json::json;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let base_url = "http://127.0.0.1:8080";

    println!("🚀 sv2d REST API Demo");
    println!("===================");

    // Test health check
    println!("\n📊 Health Check:");
    let health_response = client
        .get(&format!("{}/api/v1/health", base_url))
        .send()
        .await?;
    
    if health_response.status().is_success() {
        let health: serde_json::Value = health_response.json().await?;
        println!("✅ Status: {}", health["status"]);
        println!("   Version: {}", health["version"]);
        println!("   Uptime: {} seconds", health["uptime"]);
    } else {
        println!("❌ Health check failed: {}", health_response.status());
    }

    // Test daemon status
    println!("\n📈 Daemon Status:");
    let status_response = client
        .get(&format!("{}/api/v1/status", base_url))
        .send()
        .await?;
    
    if status_response.status().is_success() {
        let status: serde_json::Value = status_response.json().await?;
        println!("✅ Connections: {}", status["connections"]);
        println!("   Total Shares: {}", status["total_shares"]);
        println!("   Valid Shares: {}", status["valid_shares"]);
        println!("   Blocks Found: {}", status["blocks_found"]);
        println!("   Hashrate: {:.2} GH/s", status["hashrate"].as_f64().unwrap_or(0.0) / 1e9);
    } else {
        println!("❌ Status check failed: {}", status_response.status());
    }

    // Test connections endpoint
    println!("\n🔗 Active Connections:");
    let connections_response = client
        .get(&format!("{}/api/v1/connections", base_url))
        .send()
        .await?;
    
    if connections_response.status().is_success() {
        let connections: serde_json::Value = connections_response.json().await?;
        let connection_count = connections.as_array().map(|arr| arr.len()).unwrap_or(0);
        println!("✅ Found {} active connections", connection_count);
        
        if let Some(connections_array) = connections.as_array() {
            for (i, conn) in connections_array.iter().enumerate() {
                println!("   Connection {}: {} ({})", 
                    i + 1, 
                    conn["address"], 
                    conn["protocol"]
                );
            }
        }
    } else {
        println!("❌ Connections check failed: {}", connections_response.status());
    }

    // Test shares endpoint
    println!("\n📊 Recent Shares:");
    let shares_response = client
        .get(&format!("{}/api/v1/shares?limit=5", base_url))
        .send()
        .await?;
    
    if shares_response.status().is_success() {
        let shares: serde_json::Value = shares_response.json().await?;
        let share_count = shares.as_array().map(|arr| arr.len()).unwrap_or(0);
        println!("✅ Found {} recent shares", share_count);
        
        if let Some(shares_array) = shares.as_array() {
            for (i, share) in shares_array.iter().enumerate() {
                println!("   Share {}: nonce={}, difficulty={}, valid={}", 
                    i + 1, 
                    share["nonce"], 
                    share["difficulty"],
                    share["is_valid"]
                );
            }
        }
    } else {
        println!("❌ Shares check failed: {}", shares_response.status());
    }

    // Test share statistics
    println!("\n📈 Share Statistics:");
    let stats_response = client
        .get(&format!("{}/api/v1/shares/stats", base_url))
        .send()
        .await?;
    
    if stats_response.status().is_success() {
        let stats: serde_json::Value = stats_response.json().await?;
        println!("✅ Total Shares: {}", stats["total_shares"]);
        println!("   Valid Shares: {}", stats["valid_shares"]);
        println!("   Invalid Shares: {}", stats["invalid_shares"]);
        println!("   Blocks Found: {}", stats["blocks_found"]);
        println!("   Acceptance Rate: {:.2}%", stats["acceptance_rate"]);
    } else {
        println!("❌ Share stats check failed: {}", stats_response.status());
    }

    // Test mining statistics
    println!("\n⛏️  Mining Statistics:");
    let mining_stats_response = client
        .get(&format!("{}/api/v1/mining/stats", base_url))
        .send()
        .await?;
    
    if mining_stats_response.status().is_success() {
        let mining_stats: serde_json::Value = mining_stats_response.json().await?;
        println!("✅ Hashrate: {:.2} TH/s", mining_stats["hashrate"].as_f64().unwrap_or(0.0) / 1e12);
        println!("   Shares/min: {:.2}", mining_stats["shares_per_minute"]);
        println!("   Acceptance Rate: {:.2}%", mining_stats["acceptance_rate"]);
        println!("   Efficiency: {:.2}%", mining_stats["efficiency"]);
    } else {
        println!("❌ Mining stats check failed: {}", mining_stats_response.status());
    }

    // Test alerts endpoint
    println!("\n🚨 System Alerts:");
    let alerts_response = client
        .get(&format!("{}/api/v1/alerts", base_url))
        .send()
        .await?;
    
    if alerts_response.status().is_success() {
        let alerts: serde_json::Value = alerts_response.json().await?;
        let alert_count = alerts.as_array().map(|arr| arr.len()).unwrap_or(0);
        println!("✅ Found {} system alerts", alert_count);
        
        if let Some(alerts_array) = alerts.as_array() {
            for (i, alert) in alerts_array.iter().enumerate() {
                println!("   Alert {}: {} - {} ({})", 
                    i + 1, 
                    alert["level"], 
                    alert["title"],
                    alert["component"]
                );
            }
        }
    } else {
        println!("❌ Alerts check failed: {}", alerts_response.status());
    }

    // Test configuration endpoint
    println!("\n⚙️  Configuration:");
    let config_response = client
        .get(&format!("{}/api/v1/config", base_url))
        .send()
        .await?;
    
    if config_response.status().is_success() {
        let config: serde_json::Value = config_response.json().await?;
        println!("✅ Mode: {:?}", config["mode"]["type"]);
        println!("   Network: {}", config["network"]["bind_address"]);
        println!("   Max Connections: {}", config["network"]["max_connections"]);
        println!("   Database: {}", config["database"]["url"]);
    } else {
        println!("❌ Config check failed: {}", config_response.status());
    }

    // Test custom template submission
    println!("\n🔧 Custom Template Submission:");
    let template_request = json!({
        "transactions": [
            "0100000001000000000000000000000000000000000000000000000000000000000000000000000000ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000"
        ],
        "coinbase_data": "demo template",
        "difficulty": 1.0
    });

    let template_response = client
        .post(&format!("{}/api/v1/templates/custom", base_url))
        .json(&template_request)
        .send()
        .await?;
    
    if template_response.status().is_success() {
        let template: serde_json::Value = template_response.json().await?;
        println!("✅ Template created with ID: {}", template["id"]);
        println!("   Difficulty: {}", template["difficulty"]);
        println!("   Transactions: {}", template["transactions"].as_array().map(|arr| arr.len()).unwrap_or(0));
    } else {
        println!("❌ Template submission failed: {}", template_response.status());
    }

    println!("\n🎉 API Demo completed!");
    println!("\n💡 To see real-time updates, connect to the WebSocket at ws://127.0.0.1:8080/ws");
    println!("   Send: {{\"action\": \"Subscribe\", \"events\": [\"status\", \"connection\", \"share\"]}}");

    Ok(())
}