# Project Notes

## Working Rule

Keep this project simple. Prefer the smallest implementation that moves the roadmap forward, matches existing patterns, and avoids speculative abstractions.

## Current Shape

- Python package: `src/stock_portfolio_ai/`
- Rust gateway: `gateway/`
- Tests: `tests/`
- Primary docs: `README.md`, `ROADMAP.md`, `BUILD_LOG_*.md`, `PROJECT_MEMORY_BACKUP.md`

## Built

- Environment-driven OpenRouter configuration in `src/stock_portfolio_ai/config.py`.
- Market Data Agent in `src/stock_portfolio_ai/agents/market_data_agent.py`.
  - Tools: `get_stock_price`, `get_historical_data`, `get_company_news`.
  - Uses `yfinance` and can run without an LLM key for direct tool calls.
- Fundamental Analyst Agent in `src/stock_portfolio_ai/agents/fundamental_analyst_agent.py`.
  - Tools: `get_financials`, `get_key_metrics`, `get_cash_flow`.
  - Adds simple interpretation for valuation, cash flow, profitability, dividends, and financial trends.
- Shared analyst report schema in `src/stock_portfolio_ai/reports.py`.
  - Defines `AnalystReport` and `EvidenceItem` for common agent outputs.
  - Fundamental analyst can now emit a validated report with rating, confidence, key points, risks, and evidence.
- Focused pytest coverage for both agents.
- Rust Axum gateway with Askama templates and HTMX-style partial handlers.
- Index scraping and quote fetching in `gateway/src/lib.rs`.
  - Wikipedia tables provide index constituents.
  - Index detail pages render scraped constituents immediately with `N/A` market cap and price values.
  - Quote enrichment runs through a secondary HTMX request to `GET /index/:symbol/quotes`, enriches the full index, sorts by market cap descending, and returns the top 10.
  - Yahoo `401` responses and `yfinance` retries no longer block the initial index page render.
  - Index search starts background prefetch for matching index symbols to warm constituent and quote caches before click-through.
  - Results are cached in memory for one hour.
- Gateway index detail and stock search routes now use the scraper, with mock rows as fallback.

## Not Built Yet

- Technical analyst, macro context, and news synthesis agents.
- Portfolio manager for allocation, sizing, watchlists, rebalancing, and scenario analysis.
- Supervisor orchestration layer with routing, state, guardrails, and failure handling.
- Real CLI workflow beyond the bootstrap entrypoint.
- Python agent outputs in the gateway.
- Persistence, caching strategy beyond the gateway scraper cache, tracing, and evaluation hooks.

## Known Mismatches

- The build logs mention import fixes from `langgraph.prebuilt` to `langchain.agents`, but current agent files still import `create_react_agent` from `langgraph.prebuilt`.
- A nested `stock-portfolio-ai/` directory mirrors much of the repo and may be stale or accidental duplication. It is ignored and should not be pushed.

## Next Sensible Steps

1. Add focused tests for scraper parsing using static HTML fixtures.
2. Add a visible loading/refreshed state for quote enrichment.
3. Decide the canonical runtime path: `uv` with Python 3.13, or the Docker bootstrap workaround.
4. Build the next agent only when its output has a clear consumer in the portfolio or supervisor flow.
