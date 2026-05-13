# Build Log: Gateway Scrapers and Index UI

## Purpose

This log documents the Rust gateway, index constituent scraping, quote enrichment flow, and early sector-data integration. It is intended to help future agents understand why the UI behaves the way it does and where latency currently comes from.

## Product Context

The gateway is the first user-facing surface. It lets a user search for a market index and inspect the top constituents. The MVP goal was not to build a full trading dashboard; it was to prove an index-to-stock workflow that later sector, analyst, and portfolio modules can consume.

## Implementation

Gateway source:

- `gateway/src/lib.rs`
- `gateway/src/main.rs`

Templates:

- `gateway/templates/index.html`
- `gateway/templates/index_results_fragment.html`
- `gateway/templates/index_detail.html`
- `gateway/templates/index_detail_fragment.html`
- `gateway/templates/stock_row.html`
- `gateway/templates/stock_rows_fragment.html`
- `gateway/templates/stock_search_results_fragment.html`

Reference data:

- `data/sector_overrides.csv`

## Supported Indexes

- `^GSPC` - S&P 500
- `^NDX` - Nasdaq 100
- `^NSEI` - Nifty 50
- `^FTSE` - FTSE 100
- `^N225` - Nikkei 225
- `^GDAXI` - DAX 40

## Design Decisions

- Use Rust/Axum for the gateway and Askama templates for server-rendered HTML.
- Use HTMX-style partial updates to keep the UI simple without introducing a JavaScript framework.
- Scrape index constituents from Wikipedia for MVP coverage.
- Cache scraped constituents in memory for one hour.
- Render constituents immediately with `N/A` quote fields so the page is responsive.
- Load quotes in a secondary request to `GET /index/:symbol/quotes`.
- Sort enriched results by raw market cap and render the top 10.
- Use a Python `yfinance` bridge only as a fallback when Yahoo's direct quote endpoint fails or returns no data.
- Use repo-local `.uv-cache` for Python fallback execution to avoid writing to restricted home-directory cache paths.
- Add background prefetch when index search returns results to warm constituent and quote caches before click-through.

## Sector Data Integration

The gateway now treats sector as stable reference data:

- `IndexComponent` includes `sector`.
- `StockResult` includes `sector`.
- `SectorResolver` loads symbol-to-sector mappings from `data/sector_overrides.csv`.
- Missing mappings resolve to `Unknown`.
- No sector API call is made during index page render.

This keeps sector classification separate from time-varying quote data and prepares the app for sector analytics.

## Request Flow

1. User searches for an index through `POST /search/indices`.
2. Gateway returns matching index cards and starts background prefetch.
3. User opens `GET /index/:symbol`.
4. Gateway scrapes or loads cached constituents.
5. Gateway resolves sector locally and renders rows immediately.
6. Table body calls `GET /index/:symbol/quotes`.
7. Gateway fetches quotes, sorts by market cap, preserves sector, and swaps the table body.

## Verified Behavior

- Index search returns expected cards.
- Nifty 50 page renders constituent rows quickly.
- Constituents display locally resolved sector classifications where present.
- Quote enrichment updates the table with top stocks by market cap.
- `cargo test` passes.
- A unit test verifies sector CSV resolution and `Unknown` fallback behavior.

## Known Limitations

- Wikipedia structure can change and break scraping.
- Quote enrichment can still be slow because Yahoo/yfinance are external dependencies.
- Prefetch does not yet coalesce in-flight work, so a fast user click can duplicate enrichment.
- In-memory caches reset on process restart.
- Sector coverage is currently strongest for selected US names and Nifty 50 symbols; other supported indices may show `Unknown` until reference data is expanded.
- The gateway has no persistent DB yet.

## Interview Talking Points

- The UI is intentionally progressive: render useful data first, enrich expensive fields later.
- Latency was treated as an external-data problem, not a frontend-framework problem.
- The gateway preserves user experience by falling back to mock rows or `N/A` values instead of failing hard.
- Sector reference data was added to avoid unnecessary API calls and to prepare for sector-level analytics.

## Next Decisions

- Add parser tests with static HTML fixtures.
- Add visible loading/refreshed metadata for quote enrichment.
- Add in-flight request coalescing for quote enrichment.
- Expand sector reference data for all supported indices.
- Introduce SQLite/Postgres persistence when sector snapshots, quote history, and portfolio workflows require durable storage.
