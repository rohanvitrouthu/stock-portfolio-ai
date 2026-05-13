# Project Notes

## Working Rule

Keep this project simple. Prefer the smallest implementation that moves the roadmap forward, matches existing patterns, and avoids speculative abstractions.

## Current Shape

- Python package: `src/stock_portfolio_ai/`
- Rust gateway: `gateway/`
- Tests: `tests/`
- Primary docs: `README.md`, `ROADMAP.md`, `docs/DATA_MODEL.md`, `build_logs/`

## Built

- Environment-driven OpenRouter configuration in `src/stock_portfolio_ai/config.py`.
- Market Data Agent in `src/stock_portfolio_ai/agents/market_data_agent.py`.
  - Tools: `get_stock_price`, `get_historical_data`, `get_company_news`.
  - Uses `yfinance` and can run without an LLM key for direct tool calls.
- Fundamental Analyst Agent in `src/stock_portfolio_ai/agents/fundamental_analyst_agent.py`.
  - Tools: `get_financials`, `get_key_metrics`, `get_cash_flow`.
  - Adds simple interpretation for valuation, cash flow, profitability, dividends, and financial trends.
- Technical Analyst Agent in `src/stock_portfolio_ai/agents/technical_analyst_agent.py`.
  - Tool: `get_technical_indicators`.
  - Computes moving averages, RSI, recent returns, volume context, and annualized volatility.
- Shared analyst report schema in `src/stock_portfolio_ai/reports.py`.
  - Defines `AnalystReport` and `EvidenceItem` for common agent outputs.
  - Fundamental and technical analysts can now emit validated reports with rating, confidence, key points, risks, and evidence.
- Focused pytest coverage for both agents.
- Rust Axum gateway with Askama templates and HTMX-style partial handlers.
- Index scraping and quote fetching in `gateway/src/lib.rs`.
  - Wikipedia tables provide index constituents.
  - Local sector reference data in `data/sector_overrides.csv` enriches constituents without per-render sector API calls.
  - Index detail pages render scraped constituents immediately with `N/A` market cap and price values.
  - Quote enrichment runs through a secondary HTMX request to `GET /index/:symbol/quotes`, enriches the full index, sorts by market cap descending, and returns the top 10.
  - Yahoo `401` responses and `yfinance` retries no longer block the initial index page render.
  - Index search starts background prefetch for matching index symbols to warm constituent and quote caches before click-through.
  - Results are cached in memory for one hour.
- Gateway index detail and stock search routes now use the scraper, with mock rows as fallback.
- Common data model documented in `docs/DATA_MODEL.md`.

## Not Built Yet

- Macro context and news synthesis agents.
- Portfolio manager for allocation, sizing, watchlists, rebalancing, and scenario analysis.
- Supervisor orchestration layer with routing, state, guardrails, and failure handling.
- Real CLI workflow beyond the bootstrap entrypoint.
- Python agent outputs in the gateway.
- Persistence beyond local sector reference data, caching strategy beyond the gateway scraper cache, tracing, and evaluation hooks.

## Known Mismatches

- The build logs mention import fixes from `langgraph.prebuilt` to `langchain.agents`, but current agent files still import `create_react_agent` from `langgraph.prebuilt`.
- A nested `stock-portfolio-ai/` directory mirrors much of the repo and may be stale or accidental duplication. It is ignored and should not be pushed.

## Next Sensible Steps

1. Add focused tests for scraper parsing using static HTML fixtures.
2. Add a visible loading/refreshed state for quote enrichment.
3. Decide the canonical runtime path: `uv` with Python 3.13, or the Docker bootstrap workaround.
4. Add macro context or news synthesis only once the MVP has a concrete consumer for those reports.
