## Stock Portfolio AI

`stock-portfolio-ai` is a Python project for building an AI-assisted investment research and portfolio workflow. The repository requires Python 3.13 or newer; the current local `.python-version` is Python 3.14.4. The repository is structured to support:

- market and fundamentals data ingestion
- analyst-style research agents
- portfolio construction and monitoring
- a supervisor layer for orchestration
- a future UI and CLI for interacting with the system

The project uses `uv` for Python environment management and dependency installation. Runtime configuration is environment-driven, with an OpenRouter-focused configuration layer that supports simple model overrides without changing application code.

## Initial Stack

- `langchain` for LLM integration patterns
- `langgraph` for multi-agent orchestration
- `pandas` for tabular analysis
- `pydantic` for validated report schemas
- `yfinance` for market data access
- `python-dotenv` for local environment loading
- Rust, Axum, Askama, and HTMX for the gateway service

## Implemented Python Components

The Market Data Agent is implemented in [`src/stock_portfolio_ai/agents/market_data_agent.py`](src/stock_portfolio_ai/agents/market_data_agent.py). It exposes three LangChain tools backed by `yfinance`:

- `get_stock_price(symbol)`
- `get_historical_data(symbol, period="1mo")`
- `get_company_news(symbol)`

`MarketDataAgent` uses the existing settings in [`src/stock_portfolio_ai/config.py`](src/stock_portfolio_ai/config.py) to connect to the configured OpenRouter model when `OPENROUTER_API_KEY` is present. If the API key is not set, the tools still work directly and the agent returns a clear configuration error instead of failing silently.

The Fundamental Analyst Agent is implemented in [`src/stock_portfolio_ai/agents/fundamental_analyst_agent.py`](src/stock_portfolio_ai/agents/fundamental_analyst_agent.py). It exposes three LangChain tools backed by `yfinance`:

- `get_financials(symbol)`
- `get_key_metrics(symbol)`
- `get_cash_flow(symbol)`

`FundamentalAnalystAgent` uses the same configuration layer and adds built-in interpretation logic for valuation, balance-sheet context, dividend profile, and simple year-over-year fundamental trends.

Shared analyst report models live in [`src/stock_portfolio_ai/reports.py`](src/stock_portfolio_ai/reports.py):

- `AnalystReport`
- `EvidenceItem`
- common rating and agent type literals

The fundamental analyst can emit a validated `AnalystReport` through `analyze_symbol_report(symbol)`. This gives future technical, macro, news, portfolio, and supervisor components a consistent machine-readable contract instead of relying on free-form text.

## Configuration

Copy `.env.example` to `.env` and set:

```env
OPENROUTER_API_KEY=your_openrouter_api_key_here
```

The application configuration lives in [`src/stock_portfolio_ai/config.py`](src/stock_portfolio_ai/config.py). It reads environment variables by default and also allows per-run model overrides in code.

## Getting Started

```bash
uv sync
uv run stock-portfolio-ai
```

To run the agent demo tests:

```bash
UV_CACHE_DIR=.uv-cache uv run --with pytest pytest tests/test_reports.py tests/test_market_data_agent.py tests/test_fundamental_analyst_agent.py
```

## Gateway Service

The gateway is a Rust Axum web service in [`gateway/`](gateway). It serves the index search UI and uses internet scrapers to populate index constituents and market data.

Before running it, install both toolchains:

- Rust/Cargo
- `uv` for the Python fallback path used by `yfinance`

From the repository root:

```bash
uv sync
cd gateway
cargo run
```

The service listens on:

```text
http://127.0.0.1:3000
```

Useful routes:

- `GET /` opens the index search UI.
- `GET /index/%5EGSPC` opens the S&P 500 detail page.
- `GET /index/%5ENDX` opens the Nasdaq 100 detail page.
- `GET /index/%5ENSEI` opens the Nifty 50 detail page.
- `POST /search/stocks/:symbol` searches scraped constituents for an index.

The gateway currently scrapes Wikipedia for index constituents and renders rows immediately. The table body then makes a second HTMX request to enrich the full index with market cap and price data, sort by market cap descending, and replace the table with the top 10 stocks. This keeps index navigation responsive even when Yahoo Finance or `yfinance` is slow. If quote enrichment fails, the constituent rows remain visible with `N/A` quote fields; if constituent scraping fails, the UI falls back to mock rows instead of crashing.

When index search results are returned, the gateway also starts a background prefetch for matching indices. This warms the constituent and quote caches before the user clicks, reducing the visible wait on common flows. Remaining lag is usually from external data sources, not from Axum itself.

To verify the gateway without opening a browser:

```bash
cd gateway
cargo build
cargo test
cargo run
```

In another terminal:

```bash
curl http://127.0.0.1:3000/index/%5EGSPC
```

See [`ROADMAP.md`](ROADMAP.md) for the phased build plan.
