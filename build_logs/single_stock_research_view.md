# Single Stock Research View

## Purpose

The Single Stock Research view presents the four sub-agent outputs and the supervisor conclusion after a user clicks a specific stock symbol.

## Product Context

Index pages should stay fast and should not trigger expensive research work for every constituent. Research is generated only after the user chooses a stock.

## Implementation

Added gateway routes:

- `GET /stock/:symbol`
- `GET /stock/:symbol/research/:section`
- `GET /settings`

Added templates:

- `stock_detail.html`
- `stock_detail_fragment.html`
- `settings.html`
- `settings_fragment.html`
- `research_loading_fragment.html`
- `research_report_fragment.html`
- `research_market_fragment.html`
- `research_conclusion_fragment.html`
- `research_error_fragment.html`

Added configuration:

- `config/openrouter_models.json`

## Research Tabs

The stock page keeps the existing global header and renders five tabs:

- Fundamentals
- Technical
- Sentiment
- Market Data
- Conclusion

Each tab has its own loading state. HTMX loads the sections in a staged order:

1. fundamentals
2. technical
3. sentiment
4. market
5. conclusion

## OpenRouter Model Flow

The Settings page reads:

```text
config/openrouter_models.json
```

The file contains:

- `default_model`
- allowed/displayed model options

The gateway passes the configured default model to the Python research bridge. The bridge passes that model to each sub-agent and the supervisor.

The current `analyze_symbol_report()` implementations remain deterministic, so the selected model mainly affects LLM-backed `run()` paths and future LLM judging/synthesis work.

## Python Bridge

The gateway calls Python only for the requested stock and research section. It does not run research during index prefetch.

Sections:

- `fundamentals` calls `FundamentalAnalystAgent.analyze_symbol_report()`
- `technical` calls `TechnicalAnalystAgent.analyze_symbol_report()`
- `sentiment` calls `SentimentAnalystAgent.analyze_symbol_report()`
- `market` calls `MarketDataAgent` tools
- `conclusion` calls `SupervisorAgent.analyze_symbol()`

## Verification

Rust:

```bash
cargo test
```

Python:

```bash
UV_CACHE_DIR=.uv-cache uv run --with pytest pytest tests
```

Live gateway checks:

- `/settings`
- `/stock/MSFT`
- `/stock/MSFT/research/fundamentals`
- `/stock/MSFT/research/technical`
- `/stock/MSFT/research/sentiment`
- `/stock/MSFT/research/market`
- `/stock/MSFT/research/conclusion`

## Known Limitations

- The UI selector reflects config but does not persist model changes from the browser.
- Research tabs currently call Python independently, so the supervisor can repeat some data fetches already used by earlier tabs.
- LLM-based judging is not implemented yet; Codex is currently reviewing behavior during development.
- The configured model list is intentionally editable because OpenRouter model availability changes.
