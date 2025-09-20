use sv2_core::{Result, Connection, Share, ShareResult, WorkTemplate, ModeHandler, MiningStats};
use sv2_core::bitcoin_rpc::{GetBlockTemplateResponse, NetworkInfoResponse, BlockchainInfoResponse, SubmitBlockResponse, BlockTemplateTransaction};
use async_trait::async_trait;
use std::collections::HashMap;

/// Mock mode handler for testing
pub struct MockModeHandler {
    pub should_fail: bool,
}

impl MockModeHandler {
    pub fn new() -> Self {
        Self { should_fail: false }
    }

    pub fn with_failure(mut self) -> Self {
        self.should_fail = true;
        self
    }
}

#[async_trait]
impl ModeHandler for MockModeHandler {
    async fn start(&self) -> Result<()> {
        if self.should_fail {
            Err(sv2_core::Error::System("Mock start failure".to_string()))
        } else {
            Ok(())
        }
    }

    async fn stop(&self) -> Result<()> {
        if self.should_fail {
            Err(sv2_core::Error::System("Mock stop failure".to_string()))
        } else {
            Ok(())
        }
    }

    async fn handle_connection(&self, _conn: Connection) -> Result<()> {
        if self.should_fail {
            Err(sv2_core::Error::Connection("Mock failure".to_string()))
        } else {
            Ok(())
        }
    }

    async fn process_share(&self, _share: Share) -> Result<ShareResult> {
        if self.should_fail {
            Ok(ShareResult::Invalid("Mock invalid share".to_string()))
        } else {
            Ok(ShareResult::Valid)
        }
    }

    async fn get_work_template(&self) -> Result<WorkTemplate> {
        if self.should_fail {
            Err(sv2_core::Error::Template("Mock template failure".to_string()))
        } else {
            // Create a mock work template
            use bitcoin::{BlockHash, Transaction};
            use bitcoin::absolute::LockTime;
            Ok(WorkTemplate::new(
                "0000000000000000000000000000000000000000000000000000000000000000".parse().unwrap(),
                Transaction {
                    version: 1,
                    lock_time: LockTime::ZERO,
                    input: vec![],
                    output: vec![],
                },
                vec![],
                1.0,
            ))
        }
    }

    async fn handle_disconnection(&self, _connection_id: sv2_core::ConnectionId) -> Result<()> {
        Ok(())
    }

    async fn get_statistics(&self) -> Result<MiningStats> {
        Ok(MiningStats {
            hashrate: 1000.0,
            shares_per_minute: 10.0,
            acceptance_rate: 0.95,
            efficiency: 0.98,
            uptime: std::time::Duration::from_secs(3600),
            shares_accepted: 95,
            shares_rejected: 5,
            blocks_found: 0,
        })
    }

    fn validate_config(&self, _config: &sv2_core::config::DaemonConfig) -> Result<()> {
        Ok(())
    }
}

/// Mock Bitcoin RPC client for testing
pub struct MockBitcoinRpcClient {
    pub should_fail: bool,
    pub network_info: NetworkInfoResponse,
    pub blockchain_info: BlockchainInfoResponse,
    pub block_template: GetBlockTemplateResponse,
}

impl MockBitcoinRpcClient {
    pub fn new() -> Self {
        Self {
            should_fail: false,
            network_info: Self::default_network_info(),
            blockchain_info: Self::default_blockchain_info(),
            block_template: Self::default_block_template(),
        }
    }

    pub fn with_failure(mut self) -> Self {
        self.should_fail = true;
        self
    }

    fn default_network_info() -> NetworkInfoResponse {
        NetworkInfoResponse {
            version: 240000,
            subversion: "/Satoshi:24.0.0/".to_string(),
            protocolversion: 70016,
            localservices: "0000000000000409".to_string(),
            localrelay: true,
            timeoffset: 0,
            networkactive: true,
            connections: 8,
            networks: vec![],
            relayfee: 0.00001,
            incrementalfee: 0.00001,
            localaddresses: vec![],
            warnings: "".to_string(),
        }
    }

    fn default_blockchain_info() -> BlockchainInfoResponse {
        BlockchainInfoResponse {
            chain: "regtest".to_string(),
            blocks: 100,
            headers: 100,
            bestblockhash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            difficulty: 1.0,
            mediantime: 1000000000,
            verificationprogress: 1.0,
            initialblockdownload: false,
            chainwork: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            size_on_disk: 1000000,
            pruned: false,
            warnings: vec![],
        }
    }

    fn default_block_template() -> GetBlockTemplateResponse {
        GetBlockTemplateResponse {
            version: 1,
            rules: vec!["segwit".to_string()],
            vbavailable: HashMap::new(),
            vbrequired: 0,
            previousblockhash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            transactions: vec![
                BlockTemplateTransaction {
                    data: "0100000001000000000000000000000000000000000000000000000000000000000000000000000000ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".to_string(),
                    txid: "b1fea52486ce0c62bb442b530a3f0132b826c74e473d1f2c220bfa78111c5082".to_string(),
                    hash: "b1fea52486ce0c62bb442b530a3f0132b826c74e473d1f2c220bfa78111c5082".to_string(),
                    depends: vec![],
                    fee: 1000,
                    sigops: 1,
                    weight: 400,
                }
            ],
            coinbaseaux: HashMap::new(),
            coinbasevalue: 5000000000,
            longpollid: None,
            target: "0000000000000000001000000000000000000000000000000000000000000000".to_string(),
            mintime: 1000000000,
            mutable: vec!["time".to_string(), "transactions".to_string(), "prevblock".to_string()],
            noncerange: "00000000ffffffff".to_string(),
            sigoplimit: 20000,
            sizelimit: 1000000,
            weightlimit: 4000000,
            curtime: 1000000000,
            bits: "1d00ffff".to_string(),
            height: 101,
            default_witness_commitment: Some("6a24aa21a9ede2f61c3f71d1defd3fa999dfa36953755c690689799962b48bebd836974e8cf9".to_string()),
        }
    }

    pub async fn test_connection(&self) -> Result<()> {
        if self.should_fail {
            Err(sv2_core::Error::BitcoinRpc("Mock connection failure".to_string()))
        } else {
            Ok(())
        }
    }

    pub async fn get_network_info(&self) -> Result<NetworkInfoResponse> {
        if self.should_fail {
            Err(sv2_core::Error::BitcoinRpc("Mock network info failure".to_string()))
        } else {
            Ok(self.network_info.clone())
        }
    }

    pub async fn get_blockchain_info(&self) -> Result<BlockchainInfoResponse> {
        if self.should_fail {
            Err(sv2_core::Error::BitcoinRpc("Mock blockchain info failure".to_string()))
        } else {
            Ok(self.blockchain_info.clone())
        }
    }

    pub async fn get_block_template(&self, _rules: Option<Vec<String>>) -> Result<GetBlockTemplateResponse> {
        if self.should_fail {
            Err(sv2_core::Error::BitcoinRpc("Mock block template failure".to_string()))
        } else {
            Ok(self.block_template.clone())
        }
    }

    pub async fn submit_block(&self, _block_hex: &str) -> Result<SubmitBlockResponse> {
        if self.should_fail {
            Err(sv2_core::Error::BitcoinRpc("Mock submit block failure".to_string()))
        } else {
            Ok(SubmitBlockResponse::Success(None))
        }
    }

    pub async fn generate_work_template(&self, _coinbase_address: &str) -> Result<WorkTemplate> {
        if self.should_fail {
            Err(sv2_core::Error::BitcoinRpc("Mock work template failure".to_string()))
        } else {
            use bitcoin::{BlockHash, Transaction};
            use bitcoin::absolute::LockTime;
            Ok(WorkTemplate::new(
                "0000000000000000000000000000000000000000000000000000000000000000".parse().unwrap(),
                Transaction {
                    version: 1,
                    lock_time: LockTime::ZERO,
                    input: vec![bitcoin::TxIn::default()],
                    output: vec![bitcoin::TxOut::default()],
                },
                vec![],
                1.0,
            ))
        }
    }
}