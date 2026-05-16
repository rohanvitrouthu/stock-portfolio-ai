# Supervisor Agent

## Purpose

The Supervisor Agent consolidates the four current sub-agents into one investment-facing summary:

- Market Data Agent
- Fundamental Analyst Agent
- Technical Analyst Agent
- Sentiment Analyst Agent

## Product Context

Individual reports are useful for auditability, but users need a clear top-level conclusion. The supervisor turns separate evidence streams into one conclusion while preserving the underlying analyst reports for inspection.

This is not a portfolio optimizer yet. It is a single-stock research supervisor.

## Implementation

Added:

- `src/stock_portfolio_ai/agents/supervisor_agent.py`
- `tests/test_supervisor_agent.py`

Updated:

- `src/stock_portfolio_ai/reports.py`
- `src/stock_portfolio_ai/agents/__init__.py`

## Output Schema

Added `InvestmentSummary`:

- `symbol`
- `conclusion`
- `confidence`
- `summary`
- `market_data`
- `analyst_reports`
- `key_findings`
- `risks`
- `conflicts`
- `next_steps`

Conclusion values:

- `bullish`
- `neutral`
- `bearish`
- `insufficient_data`

## Consolidation Logic

The supervisor:

1. Fetches market price and recent company news from `MarketDataAgent`.
2. Fetches shared-schema analyst reports from fundamental, technical, and sentiment agents.
3. Scores bullish, neutral, and bearish ratings with agent weights.
4. Scales confidence by report confidence and evidence coverage.
5. Penalizes direct bullish-versus-bearish conflicts.
6. Surfaces missing data and opposing views as explicit conflicts.

Initial weights:

- fundamental: 40%
- technical: 30%
- sentiment: 20%

Market data is used as context rather than as a directional rating.

## UI Implication

The frontend can render `InvestmentSummary` as:

- top-level conclusion banner
- confidence indicator
- market snapshot strip
- key findings list
- risk/conflict list
- three analyst report panels
- evidence drawer per report

The UI should not display raw JSON by default, but the schema keeps the details available for audit/debug views.

## Verification

Focused checks:

```bash
UV_CACHE_DIR=.uv-cache uv run --with pytest pytest tests/test_supervisor_agent.py tests/test_reports.py tests/test_sentiment_analyst_agent.py::test_sentiment_analyst_report_uses_shared_schema tests/test_fundamental_analyst_agent.py::test_fundamental_analyst_report_uses_shared_schema tests/test_technical_analyst_agent.py::test_technical_analyst_report_uses_shared_schema
```

Result:

- passed
- existing deprecation warnings remain for `langgraph.prebuilt.create_react_agent`

## Known Limitations

- Consolidation is heuristic and not backtested.
- Market data does not yet produce its own `AnalystReport`.
- No persistence or run history exists yet.
- The supervisor is single-stock only; portfolio-level allocation is still future work.

## Next Decisions

- Decide how to expose `InvestmentSummary` in the UI or CLI.
- Add caching and request coalescing before running the full supervisor path live.
- Add evaluation fixtures for agreement, conflict, and missing-data scenarios.
