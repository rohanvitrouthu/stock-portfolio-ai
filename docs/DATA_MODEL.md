# Common Data Model

The investment workflow should separate stable reference data from time-varying market data.
Sector classification is stable enough to load from a database or cache, so index pages should not call an external API only to discover a stock's sector.

## MVP Entities

### sectors

Canonical sector names used across all supported indices.

```sql
CREATE TABLE sectors (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    normalized_name TEXT NOT NULL UNIQUE
);
```

### stocks

One row per tradable company/security.

```sql
CREATE TABLE stocks (
    id INTEGER PRIMARY KEY,
    symbol TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    exchange TEXT,
    country TEXT,
    currency TEXT,
    sector_id INTEGER REFERENCES sectors(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

For MVP, `sector` is enough. `industry` can be added later if finer classification becomes useful.

### indices

One row per supported index.

```sql
CREATE TABLE indices (
    id INTEGER PRIMARY KEY,
    symbol TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    country TEXT,
    currency TEXT
);
```

### index_constituents

Many-to-many relationship between indices and stocks.

```sql
CREATE TABLE index_constituents (
    index_id INTEGER NOT NULL REFERENCES indices(id),
    stock_id INTEGER NOT NULL REFERENCES stocks(id),
    weight REAL,
    effective_from TEXT,
    effective_to TEXT,
    source TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (index_id, stock_id, effective_from)
);
```

Sector belongs to `stocks`, not `index_constituents`, because a company keeps the same sector even if it appears in multiple indices.

### stock_quotes

Time-varying quote and market-cap data.

```sql
CREATE TABLE stock_quotes (
    stock_id INTEGER NOT NULL REFERENCES stocks(id),
    as_of TEXT NOT NULL,
    price REAL,
    market_cap REAL,
    volume REAL,
    currency TEXT,
    source TEXT NOT NULL,
    PRIMARY KEY (stock_id, as_of)
);
```

## Current MVP Implementation

The gateway currently uses a local reference file:

```text
data/sector_overrides.csv
```

This file maps symbols to canonical sector names and is loaded when `IndexScraper` starts.
If a symbol is missing, the sector resolves to `Unknown`.

Current runtime shape:

```text
IndexComponent
- symbol
- name
- sector

StockResult
- symbol
- name
- sector
- market_cap
- price
```

## Read Path

1. Scrape or load index constituents.
2. Normalize each stock symbol.
3. Resolve sector from local reference data or DB/cache.
4. Render sector immediately with constituent rows.
5. Enrich price and market cap separately.

## Future Persistence Path

When persistence is introduced, the resolver should read in this order:

1. database/cache sector for symbol
2. `data/sector_overrides.csv`
3. external provider only for unresolved symbols
4. `Unknown`

Any externally resolved sector should be persisted so later page loads do not repeat the network call.
