# Sentiment Analyst Agent

## Purpose

The Sentiment Analyst Agent turns recent company headlines into a machine-readable news-flow signal that can be combined with fundamental and technical reports.

## Product Context

Fundamental and technical signals are not enough for a practical investor workflow. Recent headlines can explain why a stock is moving and can surface near-term risk from lawsuits, downgrades, guidance cuts, launches, upgrades, or earnings beats.

The first version is intentionally deterministic so it can run without an LLM and can be tested offline with fixtures.

## Implementation

Added:

- `src/stock_portfolio_ai/agents/sentiment_analyst_agent.py`
- `tests/test_sentiment_analyst_agent.py`

Exported:

- `SentimentAnalystAgent`
- `get_news_sentiment`

Updated:

- `src/stock_portfolio_ai/reports.py`
- `src/stock_portfolio_ai/agents/__init__.py`

## Data Source

The tool reads recent headlines from:

```text
yfinance.Ticker(symbol).news
```

It normalizes each headline into:

- title
- publisher
- link
- published_at
- sentiment
- sentiment_score
- positive_terms
- negative_terms

## Scoring

The first scoring pass uses a small positive and negative term lexicon. Each headline score is:

```text
positive term count - negative term count
```

The aggregate report includes:

- headline count
- positive headline count
- negative headline count
- neutral headline count
- average headline sentiment score

## Shared Report Contract

`SentimentAnalystAgent.analyze_symbol_report(symbol)` emits an `AnalystReport` with:

- `agent_type="sentiment"`
- `rating` based on average headline score and positive/negative mix
- `confidence` scaled by headline count
- key points describing headline mix and average tone
- risk points when the news flow is negative, mixed, or risk-oriented
- evidence from `yfinance.news`

If no headlines are available, the report returns `rating="insufficient_data"`.

## Verification

Focused checks:

```bash
UV_CACHE_DIR=.uv-cache uv run --with pytest pytest tests/test_reports.py tests/test_sentiment_analyst_agent.py::test_sentiment_analyst_report_uses_shared_schema tests/test_fundamental_analyst_agent.py::test_fundamental_analyst_report_uses_shared_schema tests/test_technical_analyst_agent.py::test_technical_analyst_report_uses_shared_schema
```

Result:

- passed
- existing deprecation warnings remain for `langgraph.prebuilt.create_react_agent`

## Known Limitations

- Lexicon scoring is intentionally simple and can miss nuance, sarcasm, negation, and entity-specific context.
- `yfinance.news` availability varies by ticker and provider.
- Headlines are treated equally; source quality and recency weighting are not implemented yet.

## Next Decisions

- Decide whether to keep sentiment as a separate agent or fold it into broader news synthesis later.
- Add caching for news requests before optimizing end-to-end latency.
- Consider replacing or augmenting lexicon scoring with an LLM or financial sentiment model once evaluation fixtures exist.
