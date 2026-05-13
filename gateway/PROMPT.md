# Gateway Notes

The gateway is a Rust Axum service for the stock index UI.

## Run

From the repository root:

```bash
uv sync
cd gateway
cargo run
```

Open:

```text
http://127.0.0.1:3000
```

Smoke-test a detail page:

```bash
curl http://127.0.0.1:3000/index/%5EGSPC
```

`%5E` is the URL-encoded form of `^`.

## Verify

```bash
cd gateway
cargo build
cargo test
```

## Current Behavior

- `GET /` renders the index search UI.
- `POST /search/indices` filters the configured index list.
- `GET /index/:symbol` renders a detail page with top stocks by market cap.
- `POST /search/stocks/:symbol` searches constituents within an index.

Index constituents are scraped from Wikipedia through `gateway/src/lib.rs`.
The detail page renders scraped constituents immediately with `N/A` quote fields, then `GET /index/:symbol/quotes` enriches the full index, sorts by market cap descending, and returns the top 10 rows.
Index search also starts a background prefetch for matching indices so constituent and quote caches can warm before the user clicks.
If constituent scraping fails, handlers render mock rows instead of failing the page.

## Supported Index Symbols

- `^GSPC` - S&P 500
- `^NDX` - Nasdaq 100
- `^NSEI` - Nifty 50
- `^FTSE` - FTSE 100
- `^N225` - Nikkei 225
- `^GDAXI` - DAX 40

## Next Work

- Add scraper parser tests using static HTML fixtures.
- Add a visible loading or refreshed state for the quote-enrichment pass.
- Replace mock fallback rows with a clearer UI status once the page has room for error states.
