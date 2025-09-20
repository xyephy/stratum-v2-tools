use crate::{Result, Error};
use crate::types::{WorkTemplate, BlockTemplate};
use crate::config::BitcoinConfig;
use bitcoin::{BlockHash, Transaction, Address, Network, ScriptBuf};
use bitcoin::address::NetworkUnchecked;
use bitcoin::hashes::Hash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

/// Bitcoin RPC client for interacting with Bitcoin Core
#[derive(Debug, Clone)]
pub struct BitcoinRpcClient {
    config: BitcoinConfig,
    client: reqwest::Client,
}

/// Bitcoin RPC request structure
#[derive(Debug, Serialize)]
struct RpcRequest {
    jsonrpc: String,
    id: String,
    method: String,
    params: serde_json::Value,
}

/// Bitcoin RPC response structure
#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    jsonrpc: String,
    id: String,
    result: Option<T>,
    error: Option<RpcError>,
}

/// Bitcoin RPC error structure
#[derive(Debug, Deserialize)]
struct RpcError {
    code: i32,
    message: String,
}

/// Block template response from getblocktemplate
#[derive(Debug, Clone, Deserialize)]
pub struct GetBlockTemplateResponse {
    pub version: u32,
    pub rules: Vec<String>,
    pub vbavailable: HashMap<String, u32>,
    pub vbrequired: u32,
    pub previousblockhash: String,
    pub transactions: Vec<BlockTemplateTransaction>,
    pub coinbaseaux: HashMap<String, String>,
    pub coinbasevalue: u64,
    pub longpollid: Option<String>,
    pub target: String,
    pub mintime: u32,
    pub mutable: Vec<String>,
    pub noncerange: String,
    pub sigoplimit: u32,
    pub sizelimit: u32,
    pub weightlimit: u32,
    pub curtime: u32,
    pub bits: String,
    pub height: u64,
    pub default_witness_commitment: Option<String>,
}

/// Transaction in block template
#[derive(Debug, Clone, Deserialize)]
pub struct BlockTemplateTransaction {
    pub data: String,
    pub txid: String,
    pub hash: String,
    pub depends: Vec<u32>,
    pub fee: u64,
    pub sigops: u32,
    pub weight: u32,
}

/// Network info response
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkInfoResponse {
    pub version: u32,
    pub subversion: String,
    pub protocolversion: u32,
    pub localservices: String,
    pub localrelay: bool,
    pub timeoffset: i32,
    pub networkactive: bool,
    pub connections: u32,
    pub networks: Vec<NetworkDetails>,
    pub relayfee: f64,
    pub incrementalfee: f64,
    pub localaddresses: Vec<LocalAddress>,
    pub warnings: String,
}

/// Network details
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkDetails {
    pub name: String,
    pub limited: bool,
    pub reachable: bool,
    pub proxy: String,
    pub proxy_randomize_credentials: bool,
}

/// Local address info
#[derive(Debug, Clone, Deserialize)]
pub struct LocalAddress {
    pub address: String,
    pub port: u16,
    pub score: u32,
}

/// Blockchain info response
#[derive(Debug, Clone, Deserialize)]
pub struct BlockchainInfoResponse {
    pub chain: String,
    pub blocks: u64,
    pub headers: u64,
    pub bestblockhash: String,
    pub difficulty: f64,
    pub mediantime: u32,
    pub verificationprogress: f64,
    pub initialblockdownload: bool,
    pub chainwork: String,
    pub size_on_disk: u64,
    pub pruned: bool,
    pub warnings: Vec<String>,
}

/// Submit block response
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SubmitBlockResponse {
    Success(Option<serde_json::Value>),
    Error(String),
}

impl BitcoinRpcClient {
    /// Create a new Bitcoin RPC client
    pub fn new(config: BitcoinConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.block_template_timeout))
            .build()
            .expect("Failed to create HTTP client");

        Self { config, client }
    }

    /// Test connection to Bitcoin node
    pub async fn test_connection(&self) -> Result<()> {
        let _info = self.get_network_info().await?;
        Ok(())
    }

    /// Get network information from Bitcoin node
    pub async fn get_network_info(&self) -> Result<NetworkInfoResponse> {
        let response = self.call_rpc("getnetworkinfo", serde_json::Value::Array(vec![])).await?;
        Ok(response)
    }

    /// Get blockchain information
    pub async fn get_blockchain_info(&self) -> Result<BlockchainInfoResponse> {
        let response = self.call_rpc("getblockchaininfo", serde_json::Value::Array(vec![])).await?;
        Ok(response)
    }

    /// Get block template for mining
    pub async fn get_block_template(&self, rules: Option<Vec<String>>) -> Result<GetBlockTemplateResponse> {
        let mut params = serde_json::Map::new();
        
        // Set template request mode
        params.insert("mode".to_string(), serde_json::Value::String("template".to_string()));
        
        // Add rules if provided
        if let Some(rules) = rules {
            params.insert("rules".to_string(), serde_json::Value::Array(
                rules.into_iter().map(serde_json::Value::String).collect()
            ));
        }

        let response = self.call_rpc("getblocktemplate", serde_json::Value::Object(params)).await?;
        Ok(response)
    }

    /// Submit a completed block to the network
    pub async fn submit_block(&self, block_hex: &str) -> Result<SubmitBlockResponse> {
        let params = serde_json::Value::Array(vec![
            serde_json::Value::String(block_hex.to_string())
        ]);
        
        let response = self.call_rpc("submitblock", params).await?;
        Ok(response)
    }

    /// Generate work template from Bitcoin node block template
    pub async fn generate_work_template(&self, coinbase_address: &str) -> Result<WorkTemplate> {
        let block_template = self.get_block_template(None).await?;
        
        // Parse previous block hash
        let previous_hash: BlockHash = block_template.previousblockhash.parse()
            .map_err(|e| Error::BitcoinRpc(format!("Invalid previous block hash: {}", e)))?;

        // Create coinbase transaction
        let coinbase_tx = self.create_coinbase_transaction(
            &block_template,
            coinbase_address,
        ).await?;

        // Parse transactions
        let mut transactions = Vec::new();
        for tx_data in &block_template.transactions {
            let tx_bytes = hex::decode(&tx_data.data)
                .map_err(|e| Error::BitcoinRpc(format!("Invalid transaction hex: {}", e)))?;
            
            let tx: Transaction = bitcoin::consensus::encode::deserialize(&tx_bytes)
                .map_err(|e| Error::BitcoinRpc(format!("Failed to deserialize transaction: {}", e)))?;
            
            transactions.push(tx);
        }

        // Calculate difficulty from target
        let difficulty = self.calculate_difficulty_from_target(&block_template.target)?;

        let template = WorkTemplate::new(
            previous_hash,
            coinbase_tx,
            transactions,
            difficulty,
        );

        Ok(template)
    }

    /// Create coinbase transaction for the block template
    async fn create_coinbase_transaction(
        &self,
        template: &GetBlockTemplateResponse,
        coinbase_address: &str,
    ) -> Result<Transaction> {
        use bitcoin::{TxIn, TxOut, OutPoint, Witness};

        // Parse the coinbase address
        let address: Address<NetworkUnchecked> = coinbase_address.parse()
            .map_err(|e| Error::BitcoinRpc(format!("Invalid coinbase address: {}", e)))?;
        
        let address = address.require_network(self.get_bitcoin_network())
            .map_err(|e| Error::BitcoinRpc(format!("Address network mismatch: {}", e)))?;

        // Create coinbase input (null hash, 0xffffffff index)
        let coinbase_input = TxIn {
            previous_output: OutPoint::null(),
            script_sig: self.create_coinbase_script(template.height)?,
            sequence: bitcoin::Sequence::MAX,
            witness: Witness::new(),
        };

        // Create coinbase output
        let coinbase_output = TxOut {
            value: template.coinbasevalue,
            script_pubkey: address.script_pubkey(),
        };

        // Add witness commitment if required
        let mut outputs = vec![coinbase_output];
        if let Some(commitment) = &template.default_witness_commitment {
            let commitment_bytes = hex::decode(commitment)
                .map_err(|e| Error::BitcoinRpc(format!("Invalid witness commitment: {}", e)))?;
            
            let commitment_output = TxOut {
                value: 0,
                script_pubkey: ScriptBuf::from_bytes(commitment_bytes),
            };
            outputs.push(commitment_output);
        }

        let coinbase_tx = Transaction {
            version: template.version as i32,
            lock_time: bitcoin::absolute::LockTime::from_height(template.height as u32)
                .map_err(|e| Error::BitcoinRpc(format!("Invalid block height for locktime: {}", e)))?,
            input: vec![coinbase_input],
            output: outputs,
        };

        Ok(coinbase_tx)
    }

    /// Create coinbase script with block height and extra nonce
    fn create_coinbase_script(&self, height: u64) -> Result<ScriptBuf> {
        use bitcoin::blockdata::script::Builder;

        let mut script_builder = Builder::new();
        
        // Add block height (BIP 34)
        script_builder = script_builder.push_int(height as i64);
        
        // Add extra nonce space (8 bytes)
        script_builder = script_builder.push_slice(&[0u8; 8]);
        
        // Add arbitrary data (sv2 identifier) - This proves the block was mined via sv2d
        script_builder = script_builder.push_slice(b"/sv2-stratum-v2-daemon/");

        Ok(script_builder.into_script())
    }

    /// Calculate difficulty from target string
    fn calculate_difficulty_from_target(&self, target: &str) -> Result<f64> {
        // Remove '0x' prefix if present
        let target_str = target.strip_prefix("0x").unwrap_or(target);
        
        // Parse target as big integer
        let target_bytes = hex::decode(target_str)
            .map_err(|e| Error::BitcoinRpc(format!("Invalid target hex: {}", e)))?;

        if target_bytes.len() != 32 {
            return Err(Error::BitcoinRpc("Target must be 32 bytes".to_string()));
        }

        // Convert to difficulty (simplified calculation)
        // In practice, this would use the full difficulty calculation
        let mut target_value = 0u64;
        for (i, &byte) in target_bytes.iter().take(8).enumerate() {
            target_value |= (byte as u64) << (8 * (7 - i));
        }

        // If the first 8 bytes are zero, check the next 8 bytes
        if target_value == 0 {
            for (i, &byte) in target_bytes.iter().skip(8).take(8).enumerate() {
                target_value |= (byte as u64) << (8 * (7 - i));
            }
        }

        if target_value == 0 {
            return Err(Error::BitcoinRpc("Invalid zero target".to_string()));
        }

        // Simplified difficulty calculation
        // Real implementation would use the full 256-bit arithmetic
        let max_target = 0x00000000ffff0000_u64; // Simplified max target
        let difficulty = max_target as f64 / target_value as f64;

        Ok(difficulty.max(1.0))
    }

    /// Get Bitcoin network from config
    fn get_bitcoin_network(&self) -> Network {
        match self.config.network {
            crate::config::BitcoinNetwork::Mainnet => Network::Bitcoin,
            crate::config::BitcoinNetwork::Testnet => Network::Testnet,
            crate::config::BitcoinNetwork::Signet => Network::Signet,
            crate::config::BitcoinNetwork::Regtest => Network::Regtest,
        }
    }

    /// Make RPC call to Bitcoin node
    async fn call_rpc<T>(&self, method: &str, params: serde_json::Value) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let request = RpcRequest {
            jsonrpc: "1.0".to_string(),
            id: Uuid::new_v4().to_string(),
            method: method.to_string(),
            params,
        };

        let response = timeout(
            Duration::from_secs(self.config.block_template_timeout),
            self.client
                .post(&self.config.rpc_url)
                .basic_auth(&self.config.rpc_user, Some(&self.config.rpc_password))
                .json(&request)
                .send()
        ).await
        .map_err(|_| Error::BitcoinRpc("RPC request timeout".to_string()))?
        .map_err(|e| Error::BitcoinRpc(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::BitcoinRpc(format!(
                "HTTP error {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let rpc_response: RpcResponse<T> = response
            .json()
            .await
            .map_err(|e| Error::BitcoinRpc(format!("Failed to parse JSON response: {}", e)))?;

        if let Some(error) = rpc_response.error {
            return Err(Error::BitcoinRpc(format!(
                "RPC error {}: {}",
                error.code,
                error.message
            )));
        }

        rpc_response.result.ok_or_else(|| {
            Error::BitcoinRpc("RPC response missing result".to_string())
        })
    }

    /// Validate block template before use
    pub fn validate_block_template(&self, template: &GetBlockTemplateResponse) -> Result<()> {
        // Check required fields
        if template.previousblockhash.is_empty() {
            return Err(Error::BitcoinRpc("Missing previous block hash".to_string()));
        }

        if template.coinbasevalue == 0 {
            return Err(Error::BitcoinRpc("Invalid coinbase value".to_string()));
        }

        if template.height == 0 {
            return Err(Error::BitcoinRpc("Invalid block height".to_string()));
        }

        // Validate target
        if template.target.is_empty() {
            return Err(Error::BitcoinRpc("Missing target".to_string()));
        }

        // Check time constraints
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;

        if template.curtime > current_time + 7200 {
            return Err(Error::BitcoinRpc("Block template time too far in future".to_string()));
        }

        if template.mintime > template.curtime {
            return Err(Error::BitcoinRpc("Invalid time constraints".to_string()));
        }

        Ok(())
    }

    /// Check if block template is still valid
    pub fn is_template_valid(&self, template: &GetBlockTemplateResponse) -> bool {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;

        // Template is valid for a reasonable time window
        let max_age = 300; // 5 minutes
        current_time <= template.curtime + max_age
    }
}

/// Convert GetBlockTemplateResponse to BlockTemplate
impl From<GetBlockTemplateResponse> for BlockTemplate {
    fn from(response: GetBlockTemplateResponse) -> Self {
        // This is a simplified conversion - in practice would need proper transaction parsing
        let template = WorkTemplate {
            id: Uuid::new_v4(),
            previous_hash: response.previousblockhash.parse().unwrap_or_else(|_| BlockHash::all_zeros()),
            coinbase_tx: Transaction {
                version: response.version as i32,
                lock_time: bitcoin::absolute::LockTime::ZERO,
                input: vec![],
                output: vec![],
            },
            transactions: vec![], // Would be populated from response.transactions
            difficulty: 1.0, // Would be calculated from target
            timestamp: response.curtime,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        };

        BlockTemplate {
            template,
            height: response.height,
            reward: response.coinbasevalue,
            fees: response.transactions.iter().map(|tx| tx.fee).sum(),
            weight: response.weightlimit as u64,
            sigops: response.sigoplimit as u64,
            min_time: response.mintime,
            max_time: response.curtime + 7200, // 2 hours from current time
            mutable: response.mutable,
            noncerange: response.noncerange,
            capabilities: vec!["proposal".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BitcoinConfig, BitcoinNetwork};

    fn create_test_config() -> BitcoinConfig {
        BitcoinConfig {
            rpc_url: "http://127.0.0.1:18443".to_string(),
            rpc_user: "test".to_string(),
            rpc_password: "test".to_string(),
            network: BitcoinNetwork::Regtest,
            coinbase_address: Some("bcrt1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string()),
            block_template_timeout: 30,
        }
    }

    #[test]
    fn test_client_creation() {
        let config = create_test_config();
        let client = BitcoinRpcClient::new(config.clone());
        assert_eq!(client.config.rpc_url, config.rpc_url);
    }

    #[test]
    fn test_difficulty_calculation() {
        let config = create_test_config();
        let client = BitcoinRpcClient::new(config);
        
        // Test with a known target (has non-zero value)
        let target = "0000000000000000001000000000000000000000000000000000000000000000";
        let difficulty = client.calculate_difficulty_from_target(target).unwrap();
        assert!(difficulty > 0.0);
        
        // Test with a different target
        let target2 = "00000000ffff0000000000000000000000000000000000000000000000000000";
        let difficulty2 = client.calculate_difficulty_from_target(target2).unwrap();
        assert!(difficulty2 > 0.0);
    }

    #[test]
    fn test_coinbase_script_creation() {
        let config = create_test_config();
        let client = BitcoinRpcClient::new(config);
        
        let script = client.create_coinbase_script(100).unwrap();
        assert!(!script.is_empty());
        
        // Script should contain the block height
        let script_bytes = script.as_bytes();
        assert!(script_bytes.len() > 0);
    }

    #[test]
    fn test_network_conversion() {
        let config = create_test_config();
        let client = BitcoinRpcClient::new(config);
        
        assert_eq!(client.get_bitcoin_network(), Network::Regtest);
    }

    #[test]
    fn test_template_validation() {
        let config = create_test_config();
        let client = BitcoinRpcClient::new(config);
        
        let mut template = GetBlockTemplateResponse {
            version: 1,
            rules: vec![],
            vbavailable: HashMap::new(),
            vbrequired: 0,
            previousblockhash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            transactions: vec![],
            coinbaseaux: HashMap::new(),
            coinbasevalue: 5000000000,
            longpollid: None,
            target: "0000000000000000001000000000000000000000000000000000000000000000".to_string(),
            mintime: 1000000000,
            mutable: vec![],
            noncerange: "00000000ffffffff".to_string(),
            sigoplimit: 20000,
            sizelimit: 1000000,
            weightlimit: 4000000,
            curtime: 1000000000,
            bits: "1d00ffff".to_string(),
            height: 100,
            default_witness_commitment: None,
        };
        
        assert!(client.validate_block_template(&template).is_ok());
        
        // Test invalid template
        template.coinbasevalue = 0;
        assert!(client.validate_block_template(&template).is_err());
    }

    #[tokio::test]
    async fn test_rpc_request_structure() {
        let request = RpcRequest {
            jsonrpc: "1.0".to_string(),
            id: "test".to_string(),
            method: "getblockchaininfo".to_string(),
            params: serde_json::Value::Array(vec![]),
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("getblockchaininfo"));
        assert!(json.contains("1.0"));
    }

    #[tokio::test]
    async fn test_work_template_generation_mock() {
        // This test uses mock data to verify the work template generation logic
        let config = create_test_config();
        let client = BitcoinRpcClient::new(config);
        
        // Create a mock block template response
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
            
        let mock_template = GetBlockTemplateResponse {
            version: 1,
            rules: vec!["segwit".to_string()],
            vbavailable: HashMap::new(),
            vbrequired: 0,
            previousblockhash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            transactions: vec![],
            coinbaseaux: HashMap::new(),
            coinbasevalue: 5000000000,
            longpollid: None,
            target: "00000000ffff0000000000000000000000000000000000000000000000000000".to_string(),
            mintime: current_time - 3600, // 1 hour ago
            mutable: vec!["time".to_string(), "transactions".to_string()],
            noncerange: "00000000ffffffff".to_string(),
            sigoplimit: 20000,
            sizelimit: 1000000,
            weightlimit: 4000000,
            curtime: current_time,
            bits: "1d00ffff".to_string(),
            height: 100,
            default_witness_commitment: None,
        };
        
        // Validate the mock template
        assert!(client.validate_block_template(&mock_template).is_ok());
        assert!(client.is_template_valid(&mock_template));
        
        // Test coinbase transaction creation - use a legacy address for regtest
        let coinbase_tx = client.create_coinbase_transaction(
            &mock_template,
            "2N2JD6wb56AfK4tfmM6PwdVmoYk2dCKf4Br" // P2SH address for regtest
        ).await;
        
        if let Err(ref e) = coinbase_tx {
            println!("Coinbase transaction creation failed: {}", e);
        }
        assert!(coinbase_tx.is_ok());
        let tx = coinbase_tx.unwrap();
        assert_eq!(tx.input.len(), 1);
        assert!(tx.output.len() >= 1);
        assert_eq!(tx.output[0].value, mock_template.coinbasevalue);
    }

    #[test]
    fn test_block_template_conversion() {
        let response = GetBlockTemplateResponse {
            version: 1,
            rules: vec!["segwit".to_string()],
            vbavailable: HashMap::new(),
            vbrequired: 0,
            previousblockhash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            transactions: vec![],
            coinbaseaux: HashMap::new(),
            coinbasevalue: 5000000000,
            longpollid: None,
            target: "00000000ffff0000000000000000000000000000000000000000000000000000".to_string(),
            mintime: 1000000000,
            mutable: vec!["time".to_string()],
            noncerange: "00000000ffffffff".to_string(),
            sigoplimit: 20000,
            sizelimit: 1000000,
            weightlimit: 4000000,
            curtime: 1000000000,
            bits: "1d00ffff".to_string(),
            height: 100,
            default_witness_commitment: None,
        };
        
        let block_template: BlockTemplate = response.into();
        assert_eq!(block_template.height, 100);
        assert_eq!(block_template.reward, 5000000000);
        assert!(block_template.capabilities.contains(&"proposal".to_string()));
    }
}