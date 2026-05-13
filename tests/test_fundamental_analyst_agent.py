from __future__ import annotations

from pprint import pprint

import pytest

from stock_portfolio_ai.agents.fundamental_analyst_agent import (
    FundamentalAnalystAgent,
    get_cash_flow,
    get_financials,
    get_key_metrics,
)


def _skip_if_error(payload: dict) -> None:
    if "error" in payload:
        pytest.skip(payload["error"])


def test_get_financials_demo() -> None:
    result = get_financials.invoke({"symbol": "MSFT"})
    _skip_if_error(result)
    assert result["symbol"] == "MSFT"
    assert result["income_statement"] or result["balance_sheet"]


def test_get_key_metrics_demo() -> None:
    result = get_key_metrics.invoke({"symbol": "MSFT"})
    _skip_if_error(result)
    assert result["symbol"] == "MSFT"
    assert result["metrics"]
    assert result["interpretation"]


def test_get_cash_flow_demo() -> None:
    result = get_cash_flow.invoke({"symbol": "GOOGL"})
    _skip_if_error(result)
    assert result["symbol"] == "GOOGL"
    assert result["cash_flow"]


def test_fundamental_analyst_agent_demo() -> None:
    agent = FundamentalAnalystAgent()

    tool_result = agent.invoke_tool("get_key_metrics", symbol="MSFT")
    _skip_if_error(tool_result)
    assert tool_result["symbol"] == "MSFT"

    analysis = agent.analyze_symbol("MSFT")
    _skip_if_error(analysis)
    assert analysis["symbol"] == "MSFT"
    assert analysis["interpretation"]

    agent_result = agent.run("Analyze Microsoft's fundamentals and discuss valuation.")
    if "error" in agent_result:
        pytest.skip(agent_result["error"])

    assert agent_result["final_output"]


def test_fundamental_analyst_report_uses_shared_schema(monkeypatch: pytest.MonkeyPatch) -> None:
    agent = FundamentalAnalystAgent()

    fixtures = {
        "get_financials": {
            "symbol": "MSFT",
            "income_statement": [
                {"period": "2025-06-30", "total_revenue": 245_000_000_000, "net_income": 88_000_000_000},
                {"period": "2024-06-30", "total_revenue": 211_000_000_000, "net_income": 72_000_000_000},
            ],
            "balance_sheet": [],
        },
        "get_key_metrics": {
            "symbol": "MSFT",
            "metrics": {
                "pe_ratio": 24.0,
                "forward_pe": 22.0,
                "price_to_book": 8.0,
                "dividend_yield": 0.008,
                "return_on_equity": 0.32,
                "profit_margin": 0.36,
            },
            "interpretation": [],
        },
        "get_cash_flow": {
            "symbol": "MSFT",
            "cash_flow": [
                {"period": "2025-06-30", "operating_cash_flow": 118_000_000_000, "free_cash_flow": 74_000_000_000},
                {"period": "2024-06-30", "operating_cash_flow": 102_000_000_000, "free_cash_flow": 65_000_000_000},
            ],
            "interpretation": [],
        },
    }

    def fake_invoke_tool(tool_name: str, **_: str) -> dict:
        return fixtures[tool_name]

    monkeypatch.setattr(agent, "invoke_tool", fake_invoke_tool)

    report = agent.analyze_symbol_report("msft")

    assert report.symbol == "MSFT"
    assert report.agent_type == "fundamental"
    assert report.rating == "bullish"
    assert 0 < report.confidence <= 1
    assert report.evidence
    assert report.model_dump()["evidence"][0]["source"]


if __name__ == "__main__":
    agent = FundamentalAnalystAgent()
    print("Tool demo: get_financials(MSFT)")
    pprint(agent.invoke_tool("get_financials", symbol="MSFT"))
    print("\nTool demo: get_key_metrics(MSFT)")
    pprint(agent.invoke_tool("get_key_metrics", symbol="MSFT"))
    print("\nTool demo: get_cash_flow(GOOGL)")
    pprint(agent.invoke_tool("get_cash_flow", symbol="GOOGL"))
    print("\nAgent demo: analyze_symbol('MSFT')")
    pprint(agent.analyze_symbol("MSFT"))
