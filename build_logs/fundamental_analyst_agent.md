# Build Log: Fundamental Analyst Agent

## Purpose

This log documents the Fundamental Analyst Agent, the reasoning behind its data choices, and the decisions future builders should preserve when adding new analyst agents or portfolio workflows.

## Product Context

Investors need more than market prices. A stock research workflow should explain whether a business has attractive valuation, profitability, cash generation, and financial trends. The Fundamental Analyst Agent was created to convert raw financial-statement and metric data into structured evidence that can later be combined with technical, sector, macro, and portfolio-level analysis.

## Implementation

Primary file:

- `src/stock_portfolio_ai/agents/fundamental_analyst_agent.py`

Implemented tools:

- `get_financials(symbol)`
- `get_key_metrics(symbol)`
- `get_cash_flow(symbol)`

Agent wrapper:

- `FundamentalAnalystAgent`

Shared schema integration:

- `src/stock_portfolio_ai/reports.py`
- `FundamentalAnalystAgent.analyze_symbol_report(symbol)`

Tests:

- `tests/test_fundamental_analyst_agent.py`

## Design Decisions

- Use `yfinance` financial statements and company metadata for MVP data access.
- Keep tools directly invokable so tests and non-LLM workflows do not require an API key.
- Interpret financial data deterministically before handing it to any LLM.
- Preserve the existing `analyze_symbol()` dictionary output for compatibility.
- Add `analyze_symbol_report()` as the common machine-readable output path.
- Use `AnalystReport` and `EvidenceItem` so later supervisor and portfolio components do not need to parse prose.

## Data Returned

`get_financials` returns:

- income-statement summaries
- balance-sheet summaries

`get_key_metrics` returns:

- trailing P/E
- forward P/E
- price-to-book
- dividend yield
- return on equity
- profit margin
- valuation/profitability interpretation

`get_cash_flow` returns:

- operating cash flow
- capital expenditure
- free cash flow
- investing cash flow
- financing cash flow
- cash-flow interpretation

`analyze_symbol_report` returns:

- symbol
- `agent_type="fundamental"`
- rating
- confidence
- summary
- key points
- risks
- evidence items with source labels

## Rating Logic

The current rating is intentionally simple and explainable:

- positive signals include reasonable P/E, strong return on equity, positive profit margin, and positive free cash flow
- negative signals include very high valuation, weak profitability, and negative free cash flow
- enough positive signals produce `bullish`
- enough negative signals produce `bearish`
- mixed evidence produces `neutral`
- missing evidence produces `insufficient_data`

This should be treated as a first-pass heuristic, not investment advice.

## Verification

Test file:

- `tests/test_fundamental_analyst_agent.py`

Coverage:

- live/demo retrieval for financials, metrics, and cash flow
- direct tool invocation through `FundamentalAnalystAgent`
- stubbed shared-schema report test that does not depend on live market data

Focused verification command used during development:

```bash
UV_CACHE_DIR=.uv-cache uv run --with pytest pytest tests/test_reports.py tests/test_fundamental_analyst_agent.py::test_fundamental_analyst_report_uses_shared_schema
```

## Known Limitations

- Live data tests can skip if Yahoo/yfinance data is unavailable.
- Fundamental rating logic is heuristic and should evolve with better evidence and backtesting.
- Statement labels vary by provider and issuer, so extraction may miss fields for some companies.
- The report does not yet include sector-relative valuation or peer comparison.
- No persistence exists for downloaded financial data.

## Interview Talking Points

- This agent demonstrates turning raw financial data into auditable evidence.
- The shared report schema makes analysis composable across agents.
- The design keeps deterministic analysis separate from optional LLM summarization.
- The tool layer can be replaced or upgraded without changing the report contract.

## Next Decisions

- Add peer/sector-relative comparisons.
- Improve confidence scoring with data completeness and historical accuracy.
- Persist financial statements and metrics so repeated research does not hit upstream APIs.
- Feed fundamental reports into a portfolio manager once allocation logic exists.
