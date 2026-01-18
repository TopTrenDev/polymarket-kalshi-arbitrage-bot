# Polymarket-Kalshi Arbitrage Bot 🦀

A high-performance Rust trading bot implementing advanced arbitrage strategies across Polymarket and Kalshi prediction markets.

[![Telegram](https://img.shields.io/badge/Telegram-@toptrendev_66-2CA5E0?style=for-the-badge&logo=telegram)](https://t.me/TopTrenDev_66)
[![Twitter](https://img.shields.io/badge/Twitter-@toptrendev-1DA1F2?style=for-the-badge&logo=twitter)](https://x.com/toptrendev)
[![Discord](https://img.shields.io/badge/Discord-toptrendev-5865F2?style=for-the-badge&logo=discord)](https://discord.com/users/648385188774019072)

## ⚠️ Disclaimer

**This is not a complete, production-ready codebase.** This project is continuously being improved and developed. 

- ⚠️ **Do not use directly in production** without thorough testing and review
- 🔧 **Code is subject to change** - APIs, logic, and structure may be updated
- 🐛 **May contain bugs or incomplete features** - use at your own risk
- 📚 **Intended for educational/research purposes** - adapt and test before deploying

**Use this code as a reference or starting point, not as a ready-to-deploy solution.**

## Features

### Implemented Trading Logic

✅ **Polymarket Trading Logic** - Full implementation of Polymarket's blockchain-based trading system
- Polygon network integration with ethers-rs
- USDC balance management
- Conditional token contract interactions
- Order placement and execution

✅ **Cross-Platform Arbitrage** - Advanced event matching and price discrepancy detection
- Intelligent event matching using similarity algorithms
- Real-time price monitoring across platforms
- Simultaneous trade execution

✅ **Position Management** - Comprehensive tracking and settlement system
- Real-time position monitoring
- Automatic settlement checking
- Profit/loss calculation
- Multi-platform balance tracking

✅ **API Integration** - Production-ready clients for both platforms
- Polymarket GraphQL/CLOB API
- Kalshi REST API with RSA-PSS authentication
- Error handling and retry mechanisms

## Architecture

```
src/
├── main.rs                  # Entry point & orchestration
├── lib.rs                   # Module exports
├── event.rs                 # Event data structures
├── event_matcher.rs         # Advanced event matching algorithms
├── arbitrage_detector.rs    # Multi-strategy arbitrage detection
├── bot.rs                   # Bot orchestration & strategy execution
├── clients.rs               # Polymarket & Kalshi API clients
├── trade_executor.rs        # Trade execution engine
├── position_tracker.rs      # Position tracking & management
├── settlement_checker.rs    # Automated settlement processing
└── polymarket_blockchain.rs # Polygon blockchain integration
```

## Status

🚧 **Work in Progress** - This bot is actively being developed and improved. Features may be incomplete or change.

## Setup

> **Note**: This setup is for development/testing only. Ensure you understand the code before running with real funds.

1. **Install Rust**:

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Configure `.env`** (copy from `.env.example`):

   ```bash
   POLYGON_RPC_URL=https://polygon-rpc.com
   POLYMARKET_WALLET_PRIVATE_KEY=0x...
   KALSHI_API_KEY=your_key
   KALSHI_API_SECRET=your_secret
   ```

3. **Build & Run** (for testing/development):
   ```bash
   cargo build --release
   cargo run --release
   ```

## Platforms

| Platform   | Type           | Blockchain      | Currency   |
| ---------- | -------------- | --------------- | ---------- |
| Polymarket | Decentralized  | Polygon         | USDC       |
| Kalshi     | CFTC-regulated | Solana/TRON/BSC | USD/Crypto |

## Technical Highlights

- **Rust Performance** - Zero-cost abstractions and async/await for maximum efficiency
- **Modular Design** - Clean separation of concerns, easy to extend
- **Error Handling** - Robust error management with proper propagation
- **Concurrent Execution** - Parallel strategy execution using tokio
- **Type Safety** - Strong typing throughout for reliability

## Contributing

This project is continuously improving. Contributions, suggestions, and bug reports are welcome!
