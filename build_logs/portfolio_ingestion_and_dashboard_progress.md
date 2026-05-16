# Portfolio Ingestion and Dashboard Progress

Date: 2026-05-17

This log documents the portfolio upload work, the rationale behind the current architecture, and the next steps for internet-backed enrichment. It is intended to make the project easy to explain in interviews and easy for a future Codex session to resume.

## Why This Work Was Added

The project started with index discovery, single-stock research agents, and a supervisor agent. The next product direction is a portfolio analytics workflow where users can upload investment statements from brokers or apps such as Groww, Zerodha, Upstox, and others.

The target user experience is similar to apps that parse PDFs or spreadsheets and populate dashboards automatically, but with deeper filters:

- Asset class
- AMC or mutual fund house
- Broker or source app
- Product or scheme
- Index exposure
- Stock exposure
- Sector allocation
- Market-cap allocation

The current implementation establishes the ingestion and dashboard foundation before adding LLM/search enrichment.

## Removed Supervisor Filter

The index-level Supervisor Filter was removed.

Reason:

- It ran the full supervisor analysis across many stocks in an index.
- That created many concurrent OpenRouter requests.
- It quickly hit rate limits.
- The issue would get worse for larger indices.

Current behavior:

- Index pages show constituents, sector, market cap, and price.
- Index pages no longer trigger mass supervisor or sub-agent calls.
- Single-stock research still loads lazily when a user opens one stock.

This keeps expensive LLM/agent work tied to explicit single-stock intent instead of fan-out screens.

## Research View Changes

The single-stock research tabs now use a common rating language:

- `bullish`
- `neutral`
- `bearish`

The previous supervisor conclusion values `favorable` and `unfavorable` were replaced with `bullish` and `bearish` in the shared report schema and supervisor logic.

The standalone Market Data tab was also removed. Its content is now merged into more relevant tabs:

- Price and volume data appears in Technical.
- Top recent headlines appear in Sentiment.
- Conclusion uses bullish, neutral, and bearish vocabulary.

## Portfolio Page

A Portfolio page now exists at:

```text
/portfolio
```

It supports uploading investment statement spreadsheets and rendering dashboards.

The page has three tabs:

- Mutual Funds
- Stocks
- Other Assets

Dashboards are scoped by tab. Uploading a mutual fund statement populates only the Mutual Funds tab. Uploading a stock holdings statement populates only the Stocks tab. Other Assets remains empty unless a file cannot yet be classified or a future parser supports that asset type.

## Upload Rules

The upload form supports multiple files.

Current limits:

- Up to 3 files can be submitted in one upload action.
- Files are persisted in SQLite.
- Dashboards are based on all unique files uploaded to date, not just the latest upload.
- Duplicate files are deduped by SHA-256 hash.

Earlier implementation note:

- A temporary rolling in-memory 10-file store was implemented first.
- That was replaced because dashboards must reflect all uploaded history.

## SQLite Persistence

Portfolio data is persisted locally in:

```text
data/portfolio.sqlite
```

The database file is ignored by Git through `.gitignore`.

Tables are created by the Python parsing bridge if they do not already exist.

### uploaded_files

Stores file-level metadata and dedupe identity.

Important fields:

- `id`
- `file_hash`
- `filename`
- `asset_class`
- `uploaded_at`

`file_hash` is unique and is used to prevent duplicate ingestion.

### investments

Stores normalized investment rows from uploaded files.

Important fields:

- `upload_id`
- `asset_class`
- `source_file`
- `broker`
- `instrument_name`
- `isin`
- `transaction_type`
- `quantity`
- `nav`
- `invested_value`
- `current_value`
- `pnl`
- `transaction_date`
- `amc`
- `product`
- `asset_bucket`
- `sector`
- `market_cap_bucket`
- `raw_json`

This table is the current source of truth for portfolio dashboards.

## File Classification

Classification is currently deterministic and based on detected headers.

### Mutual Fund Statements

Detected by columns such as:

- `Scheme Name`
- `Transaction Type`
- `Units`
- `NAV`
- `Amount`
- `Date`

Tested with:

```text
Mutual_Funds_Order_History_01-04-2020_16-05-2026.xlsx
```

This Groww file is a transaction/order history, not a current holdings or fund portfolio disclosure file.

Current mutual fund dashboard metrics:

- Total purchases
- Redemptions
- Net invested
- Asset split
- AMC split
- Product split
- Yearly flow
- Fund-level view

### Direct Stock Holdings

Detected by columns such as:

- `Stock Name`
- `ISIN`
- `Quantity`
- `Average buy price`
- `Buy value`
- `Closing price`
- `Closing value`
- `Unrealised P&L`

Tested with:

```text
Stocks_Holdings_Statement_7143432504_2026-03-06.xlsx
```

Current stock dashboard metrics:

- Invested value
- Closing value
- Unrealised P&L
- Asset split
- Sector allocation
- Market-cap split
- Stock-level view
- P&L buckets

Important limitation:

- Sector and market-cap buckets are currently local heuristics from stock names.
- Production should enrich by ISIN or exchange/security master data.

### Other Assets

Other Assets is a placeholder for statements that are not recognized as mutual fund or stock files.

Future assets may include:

- Debt funds
- Bonds
- Gold
- Silver
- Fixed income products
- Commodities
- Mixed statements

## Current Limitations

Mutual fund statements only show transaction-flow analytics from the uploaded file. They do not yet show true underlying holdings, sector allocation, or market-cap split.

To calculate those accurately, the app needs a second enrichment layer:

- Scheme-to-holdings data
- Factsheets or portfolio disclosures
- Holding weights
- Holding sectors
- Holding market-cap buckets
- Date or month of the disclosed portfolio

For direct stocks, the uploaded file already contains ISIN and values, but sector and market-cap classification should be enriched from authoritative external data instead of local name heuristics.

## Proposed Enrichment Architecture

Use a hybrid deterministic plus LLM/search pipeline.

The upload request should stay fast and deterministic:

1. Extract tables from uploaded files.
2. Classify the file.
3. Normalize rows into SQLite.
4. Render dashboards from the database.

Search and LLM work should run separately:

1. Identify instruments that need enrichment.
2. Queue enrichment jobs.
3. Search trusted sources.
4. Extract structured data.
5. Cache results.
6. Recompute dashboards from enriched tables.

Recommended future tables:

### security_master

For direct stocks and listed instruments.

Suggested fields:

- `isin`
- `symbol`
- `exchange`
- `company_name`
- `sector`
- `industry`
- `market_cap`
- `market_cap_bucket`
- `source`
- `as_of_date`
- `updated_at`

### mutual_fund_schemes

For normalized mutual fund identity.

Suggested fields:

- `scheme_name`
- `amc`
- `isin`
- `scheme_code`
- `category`
- `benchmark`
- `source`
- `updated_at`

### fund_holdings

For scheme-level disclosed portfolios.

Suggested fields:

- `scheme_id`
- `holding_name`
- `holding_isin`
- `weight`
- `sector`
- `market_cap_bucket`
- `as_of_date`
- `source_url`
- `source_type`
- `created_at`

### enrichment_jobs

For background work.

Suggested fields:

- `id`
- `instrument_type`
- `instrument_key`
- `status`
- `attempt_count`
- `provider`
- `last_error`
- `created_at`
- `updated_at`

## Search/LLM Strategy

The project will likely need a tool-calling LLM connected to a search provider such as Brave Search or Tavily.

Use cases:

- Broker-specific statement schema mapping when deterministic rules are uncertain.
- Finding official AMC factsheets or portfolio disclosures.
- Extracting holdings from PDFs or web pages.
- Reconciling scheme names across uploads and sources.

Guardrails:

- Do not call search/LLM during every dashboard render.
- Do not fan out uncontrolled requests.
- Cache by ISIN, scheme, source URL, and disclosure month.
- Prefer official AMC, exchange, AMFI, or trusted provider sources.
- Use deterministic calculations after enrichment data is saved.

The LLM should be a resolver and extractor, not the accountant. Totals, grouping, dedupe, and dashboard calculations should remain deterministic.

## Files Touched

Key files changed or added:

- `gateway/src/main.rs`
- `gateway/templates/portfolio.html`
- `gateway/templates/portfolio_tab_content_fragment.html`
- `gateway/templates/portfolio_dashboard_fragment.html`
- `gateway/templates/portfolio_breakdown_row.html`
- `gateway/templates/index_detail_fragment.html`
- `gateway/templates/stock_row.html`
- `gateway/templates/stock_rows_fragment.html`
- `src/stock_portfolio_ai/reports.py`
- `src/stock_portfolio_ai/agents/supervisor_agent.py`
- `tests/test_supervisor_agent.py`
- `.gitignore`
- `README.md`

## Verification Completed

Commands and checks used:

```text
cargo check
cargo test
```

Manual endpoint verification:

- Uploaded the mutual fund sample file.
- Uploaded the direct stock sample file.
- Uploaded both together.
- Re-uploaded duplicates and verified the database stayed at 2 unique files.
- Verified dashboards aggregate from SQLite.
- Verified file limit of 3 per upload request.

SQLite sanity check showed:

```text
uploaded_files: 2
investments by asset_class:
- mutual_fund
- stock
```

## Next Session Starting Point

Recommended next steps:

1. Add explicit Rust/Python tests for the portfolio parser and SQLite aggregation.
2. Move the embedded Python parsing script out of `gateway/src/main.rs` into a maintainable module or script file.
3. Add `security_master`, `fund_holdings`, and `enrichment_jobs` tables.
4. Add a search provider interface for Brave/Tavily.
5. Build asynchronous enrichment for direct stocks by ISIN.
6. Build mutual fund factsheet discovery and holdings extraction.
7. Replace local sector and market-cap heuristics with enriched data.
8. Add support for CSV and PDF uploads.
9. Add explicit broker/source detection for Groww, Zerodha, Upstox, and other statement formats.

