from __future__ import annotations

from pprint import pprint

import pytest

from stock_portfolio_ai.agents.market_data_agent import (
    MarketDataAgent,
    get_company_news,
    get_historical_data,
    get_stock_price,
)


def _skip_if_error(payload: dict) -> None:
    if "error" in payload:
        pytest.skip(payload["error"])


def test_get_stock_price_demo() -> None:
    result = get_stock_price.invoke({"symbol": "AAPL"})
    _skip_if_error(result)
    assert result["symbol"] == "AAPL"
    assert result["price"] > 0


def test_get_historical_data_demo() -> None:
    result = get_historical_data.invoke({"symbol": "TSLA", "period": "1mo"})
    _skip_if_error(result)
    assert result["symbol"] == "TSLA"
    assert result["candles"]


def test_get_company_news_demo() -> None:
    result = get_company_news.invoke({"symbol": "AAPL"})
    _skip_if_error(result)
    assert result["symbol"] == "AAPL"
    assert result["headlines"]


def test_market_data_agent_demo() -> None:
    agent = MarketDataAgent()
    tool_result = agent.invoke_tool("get_stock_price", symbol="AAPL")
    _skip_if_error(tool_result)
    assert tool_result["symbol"] == "AAPL"

    agent_result = agent.run("Get the latest stock price for AAPL.")
    if "error" in agent_result:
        pytest.skip(agent_result["error"])

    assert agent_result["final_output"]


if __name__ == "__main__":
    agent = MarketDataAgent()
    print("Tool demo: get_stock_price(AAPL)")
    pprint(agent.invoke_tool("get_stock_price", symbol="AAPL"))
    print("\nTool demo: get_historical_data(TSLA, 1mo)")
    pprint(agent.invoke_tool("get_historical_data", symbol="TSLA", period="1mo"))
    print("\nTool demo: get_company_news(AAPL)")
    pprint(agent.invoke_tool("get_company_news", symbol="AAPL"))
    print("\nAgent demo: run('Summarize recent TSLA market data')")
    pprint(agent.run("Summarize recent TSLA market data and mention any recent headlines."))
