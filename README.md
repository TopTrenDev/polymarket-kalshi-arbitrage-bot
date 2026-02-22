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
- Simultaneous trade execution between Polymarket (Polygon) and Kalshi

✅ **Gabagool Strategy** - Single-platform hedged arbitrage on Polymarket

- Detects YES/NO price imbalances on Polymarket
- Locks in profit by buying both sides when combined cost < $1.00
- Runs simultaneously with cross-platform arbitrage

✅ **Position Management** - Comprehensive tracking and settlement system

- Real-time position monitoring
- Automatic settlement checking
- Profit/loss calculation
- Multi-platform balance tracking

✅ **API Integration** - Production-ready clients for both platforms

- Polymarket GraphQL/CLOB API
- Kalshi REST API with RSA-PSS authentication
- Error handling and retry mechanisms

✅ **Kalshi monitoring (TypeScript-aligned)** - Config, best bid/ask, dry run, monitor logging

- Centralized **config** (`KALSHI_DEMO`, `KALSHI_BASE_PATH`, PEM from file or env)
- **Best bid/ask** via `GET /markets/{ticker}` and `/markets/{ticker}/orderbook`
- **Dry run**: `DRY_RUN=true` or `KALSHI_DRY_RUN=true` (no real orders)
- **Monitor logger**: 15m slot log files `logs/monitor_{YYYY-MM-DD}_{HH}-{00|15|30|45}.log`

For **Rust crates and API references** for Kalshi and Polymarket, see [docs/RUST_APIS.md](docs/RUST_APIS.md).

## Architecture

```
src/
├── main.rs                  # Entry point & dual-strategy orchestration
├── lib.rs                   # Module exports
├── config.rs                # Kalshi config (demo/prod, PEM, dry run)
├── event.rs                 # Event data structures (MarketPrices with yes_ask/no_ask)
├── event_matcher.rs         # Advanced event matching algorithms
├── arbitrage_detector.rs    # Cross-platform arbitrage detection
├── gabagool_executor.rs     # Gabagool trade execution
├── bot.rs                   # Bot orchestration & strategy execution
├── clients.rs               # Polymarket & Kalshi API clients (get_market, orderbook)
├── trade_executor.rs        # Cross-platform trade execution
├── position_tracker.rs      # Position tracking & management
├── settlement_checker.rs    # Automated settlement processing
├── monitor_logger.rs         # 15m slot log files (TypeScript-aligned)
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

2. **Configure `.env`** (create from `.env.example`):
   - **Polymarket:** `POLYGON_RPC_URL`, `POLYMARKET_WALLET_PRIVATE_KEY`
   - **Kalshi:** `KALSHI_API_ID`, `KALSHI_RSA_PRIVATE_KEY` (or `KALSHI_PRIVATE_KEY_PATH` to a PEM file)
   - **Kalshi optional:** `KALSHI_DEMO=true`, `KALSHI_BASE_PATH`, `DRY_RUN=true` / `KALSHI_DRY_RUN=true`
   - **15m crypto (optional):** `POLYMARKET_USE_GAMMA=1`, `POLYMARKET_TAG_SLUG=crypto`, `KALSHI_SERIES_TICKER`, `COIN_FILTER=btc|eth|sol`

3. **Build & Run** (for testing/development):
   ```bash
   cargo build --release
   cargo run --release
   ```

4. **Kalshi price monitor** (optional, TypeScript-aligned):
   ```bash
   cargo run --release --bin monitor
   ```
   Polls best YES/NO ask for a Kalshi market and logs to console and to `logs/monitor_{YYYY-MM-DD}_{HH}-{00|15|30|45}.log`.  
   Env: `KALSHI_MONITOR_INTERVAL_MS` (default 2000), `KALSHI_MONITOR_TICKER` (optional; else first open KXBTC15M market).  
   Ctrl+C to stop.

## Platforms

| Platform   | Type           | Access Method                    | Currency | Supported |
| ---------- | -------------- | -------------------------------- | -------- | --------- |
| Polymarket | Decentralized  | Polygon (on-chain)               | USDC     | ✅ Full   |
| Kalshi     | CFTC-regulated | On-chain (Solana, data 100+ chains) + REST API | USD      | ✅ Full   |

**Note**:

- **Polymarket**: This bot interacts via **Polygon** (on-chain trading with USDC).
- **Kalshi**: Kalshi is on-chain (tokenized event contracts on **Solana**, real-time data via Pyth on 100+ chains). This bot uses **Kalshi’s REST API** only (`trading-api.kalshi.com`) for trading, not their Solana/on-chain layer.
- Cross-platform arbitrage is performed between these two platforms only.

## Trading Strategies

1. **Cross-Platform Arbitrage** (Polymarket ↔ Kalshi)
   - Matches identical events across platforms
   - Detects price discrepancies
   - Executes simultaneous trades to lock in profit

2. **Gabagool Strategy** (Polymarket only)
   - Single-platform hedged arbitrage
   - Buys both YES and NO when combined cost < $1.00
   - Guarantees profit regardless of outcome

Both strategies run **simultaneously** in parallel for maximum opportunity detection.

## Technical Highlights

- **Rust Performance** - Zero-cost abstractions and async/await for maximum efficiency
- **Modular Design** - Clean separation of concerns, easy to extend
- **Error Handling** - Robust error management with proper propagation
- **Concurrent Execution** - Parallel strategy execution using tokio
- **Type Safety** - Strong typing throughout for reliability
- **Dual Strategy Monitoring** - Simultaneous cross-platform and single-platform arbitrage

## Contributing

This project is continuously improving. Contributions, suggestions, and bug reports are welcome!
