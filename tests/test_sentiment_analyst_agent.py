from __future__ import annotations

from pprint import pprint

import pytest

from stock_portfolio_ai.agents.sentiment_analyst_agent import (
    SentimentAnalystAgent,
    get_news_sentiment,
)


def _skip_if_error(payload: dict) -> None:
    if "error" in payload:
        pytest.skip(payload["error"])


def test_get_news_sentiment_demo() -> None:
    result = get_news_sentiment.invoke({"symbol": "MSFT", "limit": 10})
    _skip_if_error(result)
    assert result["symbol"] == "MSFT"
    assert result["headlines"]
    assert result["aggregate"]["headline_count"] > 0
    assert result["interpretation"]


def test_sentiment_analyst_report_uses_shared_schema(monkeypatch: pytest.MonkeyPatch) -> None:
    agent = SentimentAnalystAgent()

    fixture = {
        "symbol": "MSFT",
        "headlines": [
            {
                "title": "Microsoft shares rise after strong cloud growth and raised outlook",
                "publisher": "Example News",
                "link": "https://example.com/msft-cloud",
                "published_at": "2026-05-13T09:00:00",
                "sentiment": "positive",
                "sentiment_score": 4,
                "positive_terms": ["growth", "raised", "strong"],
                "negative_terms": [],
            },
            {
                "title": "Analysts see risk from higher AI infrastructure spending",
                "publisher": "Example News",
                "link": "https://example.com/msft-ai",
                "published_at": "2026-05-13T10:00:00",
                "sentiment": "neutral",
                "sentiment_score": 0,
                "positive_terms": ["higher"],
                "negative_terms": ["risk"],
            },
        ],
        "aggregate": {
            "headline_count": 2,
            "positive_count": 1,
            "negative_count": 0,
            "neutral_count": 1,
            "average_score": 2.0,
        },
        "interpretation": [
            "Recent news sentiment is based on 2 headlines.",
            "Headline mix is 1 positive, 0 negative, and 1 neutral.",
            "Average headline tone is positive, suggesting supportive near-term news flow.",
        ],
    }

    def fake_invoke_tool(tool_name: str, **_: str) -> dict:
        assert tool_name == "get_news_sentiment"
        return fixture

    monkeypatch.setattr(agent, "invoke_tool", fake_invoke_tool)

    report = agent.analyze_symbol_report("msft")

    assert report.symbol == "MSFT"
    assert report.agent_type == "sentiment"
    assert report.rating == "bullish"
    assert 0 < report.confidence <= 1
    assert report.evidence
    assert report.model_dump()["evidence"][0]["source"] == "yfinance.news"


if __name__ == "__main__":
    agent = SentimentAnalystAgent()
    print("Tool demo: get_news_sentiment(MSFT)")
    pprint(agent.invoke_tool("get_news_sentiment", symbol="MSFT"))
    print("\nAgent demo: analyze_symbol_report('MSFT')")
    pprint(agent.analyze_symbol_report("MSFT").model_dump())
