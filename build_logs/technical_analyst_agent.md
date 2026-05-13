# Build Log: Technical Analyst Agent

## Purpose

This log documents the Technical Analyst Agent and how it extends the shared analyst-report pattern beyond fundamentals.

## Product Context

Fundamental analysis explains business quality and valuation, but investors also care about trend, momentum, volatility, and timing. The Technical Analyst Agent adds price-history evidence so future portfolio and sector workflows can compare fundamental strength against market behavior.

The goal is not to build a full trading system. The MVP goal is to produce auditable technical evidence in the same machine-readable schema used by other analysts.

## Implementation

Primary file:

- `src/stock_portfolio_ai/agents/technical_analyst_agent.py`

Implemented tool:

- `get_technical_indicators(symbol, period="6mo")`

Agent wrapper:

- `TechnicalAnalystAgent`

Shared schema integration:

- `TechnicalAnalystAgent.analyze_symbol_report(symbol)`
- `AnalystReport`
- `EvidenceItem`

Tests:

- `tests/test_technical_analyst_agent.py`

## Indicators

The tool currently computes:

- latest close
- 20-day moving average
- 50-day moving average
- 14-day RSI
- 1-month return
- 3-month return
- annualized volatility
- latest volume
- 20-day average volume

## Design Decisions

- Use daily `yfinance` price history for MVP implementation speed.
- Compute indicators deterministically in Python using `pandas`.
- Return raw indicators and interpretation together.
- Keep the technical rating heuristic simple and inspectable.
- Add a stubbed report-schema test so the core contract can be validated without relying on external market data.
- Export the agent from `stock_portfolio_ai.agents` so future orchestration code can import it consistently with existing agents.

## Rating Logic

Positive signals include:

- latest close above 20-day moving average
- 20-day moving average above 50-day moving average
- positive 1-month return above a threshold
- neutral-to-constructive RSI range

Negative signals include:

- latest close below 20-day moving average
- 20-day moving average below 50-day moving average
- weak 1-month return
- very high or very low RSI in riskier zones

The output is one of:

- `bullish`
- `neutral`
- `bearish`
- `insufficient_data`

This is a first-pass heuristic and should not be treated as a trading recommendation.

## Verification

Focused verification command used during development:

```bash
UV_CACHE_DIR=.uv-cache uv run --with pytest pytest tests/test_reports.py tests/test_fundamental_analyst_agent.py::test_fundamental_analyst_report_uses_shared_schema tests/test_technical_analyst_agent.py::test_technical_analyst_report_uses_shared_schema
```

Result:

- schema tests pass
- technical report test passes with stubbed data

## Known Limitations

- Live technical indicator demo depends on Yahoo/yfinance availability.
- The agent does not yet compute sector-relative strength.
- It does not backtest signals.
- It does not include support/resistance, breadth, beta, or drawdown.
- It currently emits a deprecation warning through LangGraph agent construction.

## Interview Talking Points

- The agent demonstrates schema reuse: fundamentals and technicals both emit `AnalystReport`.
- The implementation shows how to turn time-series data into explainable evidence.
- The design avoids black-box predictions in favor of inspectable features and ratings.
- The stubbed report test makes the core contract testable without network dependencies.

## Next Decisions

- Add sector-relative technical metrics.
- Add drawdown and beta against the parent index.
- Feed technical reports into a Sector Analyst Agent.
- Decide whether forecasting models from prior notebooks should become evidence items rather than final recommendations.
