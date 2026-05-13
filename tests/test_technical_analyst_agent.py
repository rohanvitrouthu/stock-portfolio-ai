from __future__ import annotations

from pprint import pprint

import pytest

from stock_portfolio_ai.agents.technical_analyst_agent import (
    TechnicalAnalystAgent,
    get_technical_indicators,
)


def _skip_if_error(payload: dict) -> None:
    if "error" in payload:
        pytest.skip(payload["error"])


def test_get_technical_indicators_demo() -> None:
    result = get_technical_indicators.invoke({"symbol": "AAPL", "period": "6mo"})
    _skip_if_error(result)
    assert result["symbol"] == "AAPL"
    assert result["indicators"]
    assert result["interpretation"]


def test_technical_analyst_report_uses_shared_schema(monkeypatch: pytest.MonkeyPatch) -> None:
    agent = TechnicalAnalystAgent()

    fixture = {
        "symbol": "MSFT",
        "period": "6mo",
        "indicators": {
            "latest_close": 450.0,
            "sma_20": 430.0,
            "sma_50": 410.0,
            "rsi_14": 58.0,
            "return_1m_pct": 6.2,
            "return_3m_pct": 14.5,
            "annualized_volatility_pct": 22.0,
            "latest_volume": 24_000_000,
            "avg_volume_20": 21_000_000,
        },
        "interpretation": [
            "Price is above the 20-day moving average, indicating positive short-term momentum.",
            "The 20-day moving average is above the 50-day moving average, which supports an upward trend bias.",
            "RSI of 58.0 is in a neutral range.",
            "The stock has gained 6.2% over the last month.",
        ],
    }

    def fake_invoke_tool(tool_name: str, **_: str) -> dict:
        assert tool_name == "get_technical_indicators"
        return fixture

    monkeypatch.setattr(agent, "invoke_tool", fake_invoke_tool)

    report = agent.analyze_symbol_report("msft")

    assert report.symbol == "MSFT"
    assert report.agent_type == "technical"
    assert report.rating == "bullish"
    assert 0 < report.confidence <= 1
    assert report.evidence
    assert report.model_dump()["evidence"][0]["source"] == "yfinance.history"


if __name__ == "__main__":
    agent = TechnicalAnalystAgent()
    print("Tool demo: get_technical_indicators(AAPL)")
    pprint(agent.invoke_tool("get_technical_indicators", symbol="AAPL"))
    print("\nAgent demo: analyze_symbol_report('MSFT')")
    pprint(agent.analyze_symbol_report("MSFT").model_dump())
