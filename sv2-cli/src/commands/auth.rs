use crate::client::ApiClient;
use anyhow::{Result, Error};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use sv2_core::auth::{Permission, ApiKeyInfo};
use tabled::{Table, Tabled};
use colored::*;

/// Authentication and authorization management commands
#[derive(Debug, Args)]
pub struct AuthCommand {
    #[command(subcommand)]
    pub command: AuthSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AuthSubcommand {
    /// Generate a new API key
    GenerateKey(GenerateKeyArgs),
    /// List all API keys
    ListKeys(ListKeysArgs),
    /// Revoke an API key
    RevokeKey(RevokeKeyArgs),
    /// List active sessions
    ListSessions(ListSessionsArgs),
    /// Invalidate a session
    InvalidateSession(InvalidateSessionArgs),
    /// Test API key authentication
    TestKey(TestKeyArgs),
}

#[derive(Debug, Args)]
pub struct GenerateKeyArgs {
    /// Name/description for the API key
    #[arg(short, long)]
    pub name: String,
    
    /// Permissions to grant (comma-separated)
    #[arg(short, long, value_delimiter = ',')]
    pub permissions: Vec<String>,
    
    /// Expiration time in hours (optional)
    #[arg(short, long)]
    pub expires_in: Option<u64>,
    
    /// Output format (json, table)
    #[arg(long, default_value = "table")]
    pub format: String,
}

#[derive(Debug, Args)]
pub struct ListKeysArgs {
    /// Show inactive keys
    #[arg(long)]
    pub include_inactive: bool,
    
    /// Output format (json, table)
    #[arg(long, default_value = "table")]
    pub format: String,
}

#[derive(Debug, Args)]
pub struct RevokeKeyArgs {
    /// API key ID to revoke
    pub key_id: String,
    
    /// Confirm revocation without prompt
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct ListSessionsArgs {
    /// API key ID to filter by
    #[arg(long)]
    pub key_id: Option<String>,
    
    /// Output format (json, table)
    #[arg(long, default_value = "table")]
    pub format: String,
}

#[derive(Debug, Args)]
pub struct InvalidateSessionArgs {
    /// Session ID to invalidate
    pub session_id: String,
    
    /// Confirm invalidation without prompt
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct TestKeyArgs {
    /// API key to test
    pub api_key: String,
    
    /// Endpoint to test against
    #[arg(long, default_value = "/api/v1/status")]
    pub endpoint: String,
}

/// API key generation response
#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateKeyResponse {
    pub key_id: String,
    pub api_key: String,
    pub name: String,
    pub permissions: Vec<Permission>,
    pub expires_at: Option<u64>,
}

/// Session information for display
#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct SessionDisplay {
    #[tabled(rename = "Session ID")]
    pub id: String,
    #[tabled(rename = "API Key ID")]
    pub api_key_id: String,
    #[tabled(rename = "Client ID")]
    pub client_id: String,
    #[tabled(rename = "Created")]
    pub created_at: String,
    #[tabled(rename = "Last Activity")]
    pub last_activity: String,
    #[tabled(rename = "Expires")]
    pub expires_at: String,
}

/// API key display information
#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct ApiKeyDisplay {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "Name")]
    pub name: String,
    #[tabled(rename = "Permissions")]
    pub permissions: String,
    #[tabled(rename = "Created")]
    pub created_at: String,
    #[tabled(rename = "Last Used")]
    pub last_used: String,
    #[tabled(rename = "Expires")]
    pub expires_at: String,
    #[tabled(rename = "Active")]
    pub active: String,
}

impl AuthCommand {
    pub async fn execute(&self, client: &ApiClient) -> Result<()> {
        match &self.command {
            AuthSubcommand::GenerateKey(args) => self.generate_key(client, args).await,
            AuthSubcommand::ListKeys(args) => self.list_keys(client, args).await,
            AuthSubcommand::RevokeKey(args) => self.revoke_key(client, args).await,
            AuthSubcommand::ListSessions(args) => self.list_sessions(client, args).await,
            AuthSubcommand::InvalidateSession(args) => self.invalidate_session(client, args).await,
            AuthSubcommand::TestKey(args) => self.test_key(client, args).await,
        }
    }

    async fn generate_key(&self, client: &ApiClient, args: &GenerateKeyArgs) -> Result<()> {
        // Parse permissions
        let permissions = self.parse_permissions(&args.permissions)?;
        
        // Calculate expiration timestamp
        let expires_at = args.expires_in.map(|hours| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() + (hours * 3600)
        });

        // Create request payload
        let request = serde_json::json!({
            "name": args.name,
            "permissions": permissions,
            "expires_at": expires_at
        });

        // Send request to daemon
        let response: GenerateKeyResponse = client.post("/api/v1/auth/keys", &request).await?;

        match args.format.as_str() {
            "json" => {
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
            "table" | _ => {
                println!("{}", "✅ API Key Generated Successfully".green().bold());
                println!();
                println!("{}: {}", "Key ID".bold(), response.key_id);
                println!("{}: {}", "API Key".bold(), response.api_key.yellow());
                println!("{}: {}", "Name".bold(), response.name);
                println!("{}: {:?}", "Permissions".bold(), response.permissions);
                if let Some(expires) = response.expires_at {
                    let expires_dt = chrono::DateTime::from_timestamp(expires as i64, 0)
                        .unwrap_or_default();
                    println!("{}: {}", "Expires".bold(), expires_dt.format("%Y-%m-%d %H:%M:%S UTC"));
                } else {
                    println!("{}: {}", "Expires".bold(), "Never");
                }
                println!();
                println!("{}", "⚠️  Store this API key securely - it will not be shown again!".yellow());
            }
        }

        Ok(())
    }

    async fn list_keys(&self, client: &ApiClient, args: &ListKeysArgs) -> Result<()> {
        let mut url = "/api/v1/auth/keys".to_string();
        if args.include_inactive {
            url.push_str("?include_inactive=true");
        }

        let keys: Vec<ApiKeyInfo> = client.get(&url).await?;
        
        if keys.is_empty() {
            println!("{}", "No API keys found".yellow());
            return Ok(());
        }

        match args.format.as_str() {
            "json" => {
                println!("{}", serde_json::to_string_pretty(&keys)?);
            }
            "table" | _ => {
                let display_keys: Vec<ApiKeyDisplay> = keys.into_iter()
                    .map(|key| self.format_api_key_display(key))
                    .collect();

                let table = Table::new(display_keys).to_string();
                println!("{}", table);
            }
        }

        Ok(())
    }

    async fn revoke_key(&self, client: &ApiClient, args: &RevokeKeyArgs) -> Result<()> {
        if !args.yes {
            print!("Are you sure you want to revoke API key '{}'? [y/N]: ", args.key_id);
            use std::io::{self, Write};
            io::stdout().flush().unwrap();
            
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            
            if !input.trim().to_lowercase().starts_with('y') {
                println!("Revocation cancelled");
                return Ok(());
            }
        }

        let url = format!("/api/v1/auth/keys/{}", args.key_id);
        client.delete(&url).await?;

        println!("{}", "✅ API key revoked successfully".green());
        Ok(())
    }

    async fn list_sessions(&self, client: &ApiClient, args: &ListSessionsArgs) -> Result<()> {
        let mut url = "/api/v1/auth/sessions".to_string();
        if let Some(key_id) = &args.key_id {
            url.push_str(&format!("?key_id={}", key_id));
        }

        let sessions: Vec<sv2_core::auth::SessionInfo> = client.get(&url).await?;
        
        if sessions.is_empty() {
            println!("{}", "No active sessions found".yellow());
            return Ok(());
        }

        match args.format.as_str() {
            "json" => {
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            }
            "table" | _ => {
                let display_sessions: Vec<SessionDisplay> = sessions.into_iter()
                    .map(|session| self.format_session_display(session))
                    .collect();

                let table = Table::new(display_sessions).to_string();
                println!("{}", table);
            }
        }

        Ok(())
    }

    async fn invalidate_session(&self, client: &ApiClient, args: &InvalidateSessionArgs) -> Result<()> {
        if !args.yes {
            print!("Are you sure you want to invalidate session '{}'? [y/N]: ", args.session_id);
            use std::io::{self, Write};
            io::stdout().flush().unwrap();
            
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            
            if !input.trim().to_lowercase().starts_with('y') {
                println!("Invalidation cancelled");
                return Ok(());
            }
        }

        let url = format!("/api/v1/auth/sessions/{}", args.session_id);
        client.delete(&url).await?;

        println!("{}", "✅ Session invalidated successfully".green());
        Ok(())
    }

    async fn test_key(&self, client: &ApiClient, args: &TestKeyArgs) -> Result<()> {
        println!("Testing API key authentication...");
        
        // Create a test client with the provided API key
        let test_client = ApiClient::new_with_auth(client.base_url(), Some(args.api_key.clone()))?;
        
        // Test the endpoint
        match test_client.get::<serde_json::Value>(&args.endpoint).await {
            Ok(response) => {
                println!("{}", "✅ Authentication successful".green());
                println!("Response from {}:", args.endpoint);
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
            Err(e) => {
                println!("{}", "❌ Authentication failed".red());
                println!("Error: {}", e);
            }
        }

        Ok(())
    }

    fn parse_permissions(&self, permission_strings: &[String]) -> Result<Vec<Permission>> {
        let mut permissions = Vec::new();
        
        for perm_str in permission_strings {
            let permission = match perm_str.to_lowercase().as_str() {
                "view_connections" => Permission::ViewConnections,
                "manage_connections" => Permission::ManageConnections,
                "view_shares" => Permission::ViewShares,
                "submit_shares" => Permission::SubmitShares,
                "view_templates" => Permission::ViewTemplates,
                "create_templates" => Permission::CreateTemplates,
                "manage_templates" => Permission::ManageTemplates,
                "view_config" => Permission::ViewConfig,
                "update_config" => Permission::UpdateConfig,
                "start_daemon" => Permission::StartDaemon,
                "stop_daemon" => Permission::StopDaemon,
                "restart_daemon" => Permission::RestartDaemon,
                "reload_config" => Permission::ReloadConfig,
                "view_metrics" => Permission::ViewMetrics,
                "view_health" => Permission::ViewHealth,
                "manage_alerts" => Permission::ManageAlerts,
                "api_access" => Permission::ApiAccess,
                "admin_access" => Permission::AdminAccess,
                "start_mining" => Permission::StartMining,
                "stop_mining" => Permission::StopMining,
                "view_mining_stats" => Permission::ViewMiningStats,
                "view_database" => Permission::ViewDatabase,
                "manage_database" => Permission::ManageDatabase,
                _ => {
                    return Err(anyhow::anyhow!("Unknown permission: {}", perm_str));
                }
            };
            permissions.push(permission);
        }
        
        if permissions.is_empty() {
            return Err(anyhow::anyhow!("At least one permission must be specified"));
        }
        
        Ok(permissions)
    }

    fn format_api_key_display(&self, key: ApiKeyInfo) -> ApiKeyDisplay {
        let created_dt = chrono::DateTime::from_timestamp(key.created_at as i64, 0)
            .unwrap_or_default();
        
        let last_used = key.last_used
            .map(|ts| {
                chrono::DateTime::from_timestamp(ts as i64, 0)
                    .unwrap_or_default()
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|| "Never".to_string());
        
        let expires_at = key.expires_at
            .map(|ts| {
                chrono::DateTime::from_timestamp(ts as i64, 0)
                    .unwrap_or_default()
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|| "Never".to_string());

        let permissions = key.permissions.iter()
            .map(|p| format!("{:?}", p))
            .collect::<Vec<_>>()
            .join(", ");

        ApiKeyDisplay {
            id: key.id,
            name: key.name,
            permissions,
            created_at: created_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            last_used,
            expires_at,
            active: if key.active { "Yes".to_string() } else { "No".to_string() },
        }
    }

    fn format_session_display(&self, session: sv2_core::auth::SessionInfo) -> SessionDisplay {
        let created_dt = chrono::DateTime::from_timestamp(session.created_at as i64, 0)
            .unwrap_or_default();
        let last_activity_dt = chrono::DateTime::from_timestamp(session.last_activity as i64, 0)
            .unwrap_or_default();
        let expires_dt = chrono::DateTime::from_timestamp(session.expires_at as i64, 0)
            .unwrap_or_default();

        SessionDisplay {
            id: session.id,
            api_key_id: session.api_key_id,
            client_id: session.client_id,
            created_at: created_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            last_activity: last_activity_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            expires_at: expires_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}

/// Print available permissions help
pub fn print_permissions_help() {
    println!("{}", "Available Permissions:".bold());
    println!();
    
    let permissions = [
        ("view_connections", "View active connections"),
        ("manage_connections", "Manage connections (disconnect, etc.)"),
        ("view_shares", "View share submissions"),
        ("submit_shares", "Submit shares"),
        ("view_templates", "View work templates"),
        ("create_templates", "Create custom work templates"),
        ("manage_templates", "Manage work templates"),
        ("view_config", "View daemon configuration"),
        ("update_config", "Update daemon configuration"),
        ("start_daemon", "Start daemon"),
        ("stop_daemon", "Stop daemon"),
        ("restart_daemon", "Restart daemon"),
        ("reload_config", "Reload daemon configuration"),
        ("view_metrics", "View performance metrics"),
        ("view_health", "View health status"),
        ("manage_alerts", "Manage alerts and notifications"),
        ("api_access", "General API access"),
        ("admin_access", "Full administrative access"),
        ("start_mining", "Start mining operations"),
        ("stop_mining", "Stop mining operations"),
        ("view_mining_stats", "View mining statistics"),
        ("view_database", "View database contents"),
        ("manage_database", "Manage database operations"),
    ];
    
    for (perm, desc) in permissions {
        println!("  {:<20} - {}", perm.cyan(), desc);
    }
    
    println!();
    println!("{}", "Examples:".bold());
    println!("  sv2-cli auth generate-key --name \"Read Only\" --permissions view_connections,view_shares,view_metrics");
    println!("  sv2-cli auth generate-key --name \"Admin\" --permissions admin_access --expires-in 24");
    println!("  sv2-cli auth generate-key --name \"Mining Bot\" --permissions submit_shares,view_templates");
}