use anyhow::{Context, Result};
use ethers::providers::{Http, Middleware, Provider};
use ethers::signers::{LocalWallet, Signer};
use ethers::types::{Address, H256, TransactionRequest, U256, U64};
use std::str::FromStr;

/// Legacy Polygon helpers for balance checks.
/// Trading is handled via CLOB V2 (`polymarket_clob` + official SDK).
pub struct PolymarketBlockchain {
    provider: Provider<Http>,
    wallet: Option<LocalWallet>,
    chain_id: u64,
}

impl PolymarketBlockchain {
    pub fn new(rpc_url: &str) -> Result<Self> {
        let provider = Provider::<Http>::try_from(rpc_url)
            .context("Failed to create Polygon provider")?;

        Ok(Self {
            provider,
            wallet: None,
            chain_id: 137,
        })
    }

    pub fn with_wallet(mut self, private_key: &str) -> Result<Self> {
        let wallet: LocalWallet = private_key
            .parse()
            .context("Invalid private key format. Must be hex string starting with 0x")?;

        let wallet = wallet.with_chain_id(self.chain_id);
        self.wallet = Some(wallet);

        Ok(self)
    }

    pub fn address(&self) -> Result<Address> {
        let wallet = self
            .wallet
            .as_ref()
            .context("Wallet not initialized")?;
        Ok(wallet.address())
    }

    /// Reads bridged USDC balance on Polygon (legacy collateral).
    /// CLOB V2 uses pUSD in the funder wallet; use Polymarket account APIs for trading balance.
    pub async fn get_usdc_balance(&self) -> Result<f64> {
        let address = self.address()?;
        let usdc_address: Address = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"
            .parse()
            .context("Invalid USDC contract address")?;

        let function_selector = [0x70, 0xa0, 0x82, 0x31];
        let mut data = Vec::from(function_selector);

        let mut address_bytes = [0u8; 32];
        address_bytes[12..].copy_from_slice(address.as_ref());
        data.extend_from_slice(&address_bytes);

        let result = self
            .provider
            .call(
                &TransactionRequest::new()
                    .to(usdc_address)
                    .data(data.into()),
                None,
            )
            .await
            .context("Failed to call USDC balanceOf")?;

        if result.len() >= 32 {
            let balance = U256::from_big_endian(&result[..32]);
            let balance_f64 = balance.as_u128() as f64 / 1_000_000.0;
            Ok(balance_f64)
        } else {
            Err(anyhow::anyhow!(
                "Invalid balance response from USDC contract"
            ))
        }
    }

    pub async fn check_transaction(&self, tx_hash: &str) -> Result<bool> {
        let hash = H256::from_str(tx_hash).context("Invalid transaction hash")?;

        let receipt = self
            .provider
            .get_transaction_receipt(hash)
            .await
            .context("Failed to get transaction receipt")?;

        if let Some(receipt) = receipt {
            Ok(receipt.status == Some(U64::from(1)))
        } else {
            Ok(false)
        }
    }

    pub async fn get_gas_price(&self) -> Result<U256> {
        self.provider
            .get_gas_price()
            .await
            .context("Failed to get gas price")
    }
}
