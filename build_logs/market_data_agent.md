# Build Log: Market Data Agent

## Purpose

This log explains why the Market Data Agent exists, what was built, and what future developers or agents should preserve when extending the service.

## Product Context

The project needs a reliable entry point for raw market data before higher-level analysts, portfolio managers, or supervisors can reason about stocks. The first version deliberately uses `yfinance` because it is fast to integrate, broad enough for prototyping, and already supports price history, quote metadata, and company news.

This agent is not intended to be the final market-data infrastructure. It is the MVP data access layer that proves the workflow and gives later components a stable interface.

## Implementation

Primary file:

- `src/stock_portfolio_ai/agents/market_data_agent.py`

Implemented tools:

- `get_stock_price(symbol)`
- `get_historical_data(symbol, period="1mo")`
- `get_company_news(symbol)`

Agent wrapper:

- `MarketDataAgent`

Supporting files:

- `src/stock_portfolio_ai/config.py`
- `tests/test_market_data_agent.py`

## Design Decisions

- Use LangChain `@tool` decorators so each function can be invoked directly or by an agent runtime.
- Normalize ticker symbols to uppercase and reject empty input early.
- Return structured dictionaries instead of free-form text so downstream logic can inspect values deterministically.
- Allow tool calls even when `OPENROUTER_API_KEY` is not configured.
- Return a clear configuration error from `MarketDataAgent.run()` when LLM access is unavailable instead of failing silently.
- Use `yfinance` for MVP speed, with the expectation that production-grade market data may later require a paid or more reliable provider.

## Data Returned

`get_stock_price` returns:

- symbol
- latest price
- currency
- exchange
- as-of timestamp
- OHLCV values from the latest available candle

`get_historical_data` returns:

- symbol
- requested period
- daily OHLCV candle list

`get_company_news` returns:

- symbol
- recent headlines
- publisher
- link
- published timestamp when available

## Verification

Test file:

- `tests/test_market_data_agent.py`

Coverage:

- current price retrieval for `AAPL`
- historical data retrieval for `TSLA`
- company news retrieval for `AAPL`
- direct tool invocation through `MarketDataAgent`
- graceful skip behavior when external market data is unavailable

## Known Limitations

- Tests depend on live external data and may skip if `yfinance` or Yahoo data is unavailable.
- No retry/backoff layer exists yet.
- No persistent cache exists in Python market-data tools.
- News quality and availability depend entirely on upstream provider behavior.
- `create_react_agent` currently imports from `langgraph.prebuilt`, which emits a deprecation warning in newer LangGraph versions.

## Interview Talking Points

- The agent separates data retrieval from analysis, making it easier to test and replace.
- The implementation supports both direct deterministic tool calls and LLM-backed agent execution.
- Configuration is environment-driven, so models and credentials can change without code changes.
- The MVP intentionally favors a simple provider integration over premature infrastructure.

## Next Decisions

- Add a retry/cache layer for market-data calls.
- Decide whether to keep `yfinance` as a fallback while introducing a production-grade provider.
- Convert output dictionaries into stricter Pydantic models if downstream consumers need stronger contracts.
- Update LangGraph/LangChain agent construction to remove deprecation warnings.
