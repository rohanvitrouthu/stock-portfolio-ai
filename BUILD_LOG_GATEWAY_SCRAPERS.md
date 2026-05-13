# Build Log: Gateway Scrapers and Index UI

## Objective

Build a lightweight gateway flow where a user can search for a stock market index, click the result, and see the top 10 stocks in that index by market cap.

## What Was Built

1. Added index constituent scraping in `gateway/src/lib.rs`.
   - Uses Wikipedia pages as the constituent source.
   - Supports `^GSPC`, `^NDX`, `^NSEI`, `^FTSE`, `^N225`, and `^GDAXI`.
   - Caches scraped constituents in memory for one hour.

2. Wired the scrapers into the Axum gateway in `gateway/src/main.rs`.
   - `GET /index/:symbol` renders the index detail page.
   - `POST /search/stocks/:symbol` searches constituents for an index.
   - Handlers keep mock rows as fallback if scraping fails.

3. Split page rendering from quote enrichment.
   - The index detail page renders quickly with scraped constituents and `N/A` quote fields.
   - The table body then calls `GET /index/:symbol/quotes` through HTMX.
   - The quote endpoint enriches the full index, sorts by raw market cap descending, and returns the top 10 rows.

4. Added quote lookup fallback behavior.
   - Yahoo Finance direct quote endpoint is attempted first.
   - If Yahoo rejects the request, the gateway falls back to a Python `yfinance` bridge.
   - The Python bridge runs in parallel threads and uses a repo-local `.uv-cache` to avoid home-directory cache permission issues.

5. Added cache warming.
   - `POST /search/indices` starts background prefetch for matching index symbols.
   - This warms constituent and quote caches before the user clicks an index result.

## Verified Behavior

- Index search returns expected cards.
- Clicking Nifty 50 renders a constituent table quickly.
- Quote enrichment updates the table with the top 10 Nifty stocks by market cap.
- Observed Nifty ordering after enrichment:
  - `RELIANCE.NS`
  - `HDFCBANK.NS`
  - `BHARTIARTL.NS`
  - `SBIN.NS`
  - `ICICIBANK.NS`
  - `TCS.NS`
- `cargo build` passes.
- `cargo test` passes.

## Important Notes

- The remaining lag is from external data sources, not from Rust or Axum.
- Migrating to a JavaScript framework would not remove the Wikipedia/Yahoo/yfinance network costs.
- The right performance path is caching, prefetching, and eventually a more reliable market-data provider.

## Next Steps

1. Add scraper parser tests with static HTML fixtures.
2. Add an explicit loading/refreshed state for the quote enrichment pass.
3. Add cache metadata to the UI so it is clear when rows are fresh or fallback.
4. Consider persistent cache storage if cold-start latency remains annoying.
