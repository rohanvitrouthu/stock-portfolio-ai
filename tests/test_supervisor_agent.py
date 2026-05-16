from __future__ import annotations

from stock_portfolio_ai.agents.supervisor_agent import SupervisorAgent
from stock_portfolio_ai.reports import AnalystReport


class StubMarketDataAgent:
    def invoke_tool(self, tool_name: str, **_: str) -> dict:
        if tool_name == "get_stock_price":
            return {
                "symbol": "MSFT",
                "price": 450.0,
                "currency": "USD",
                "exchange": "NMS",
            }
        if tool_name == "get_company_news":
            return {
                "symbol": "MSFT",
                "headlines": [
                    {"title": "Microsoft shares rise after strong cloud growth"},
                    {"title": "Analysts watch AI infrastructure spending"},
                ],
            }
        raise ValueError(f"Unexpected tool: {tool_name}")


class StubAnalystAgent:
    def __init__(self, report: AnalystReport) -> None:
        self.report = report

    def analyze_symbol_report(self, symbol: str) -> AnalystReport:
        assert symbol == "MSFT"
        return self.report


def test_supervisor_consolidates_four_agent_outputs() -> None:
    supervisor = SupervisorAgent(
        market_data_agent=StubMarketDataAgent(),  # type: ignore[arg-type]
        fundamental_agent=StubAnalystAgent(
            AnalystReport(
                symbol="MSFT",
                agent_type="fundamental",
                rating="bullish",
                confidence=0.80,
                summary="Fundamentals are supported by margins and cash flow.",
                risks=["Valuation is elevated."],
            )
        ),  # type: ignore[arg-type]
        technical_agent=StubAnalystAgent(
            AnalystReport(
                symbol="MSFT",
                agent_type="technical",
                rating="bullish",
                confidence=0.70,
                summary="Momentum remains positive.",
            )
        ),  # type: ignore[arg-type]
        sentiment_agent=StubAnalystAgent(
            AnalystReport(
                symbol="MSFT",
                agent_type="sentiment",
                rating="neutral",
                confidence=0.55,
                summary="Headlines are mixed but not negative.",
                risks=["Average headline tone is mixed or neutral."],
            )
        ),  # type: ignore[arg-type]
    )

    summary = supervisor.analyze_symbol("msft")

    assert summary.symbol == "MSFT"
    assert summary.conclusion == "bullish"
    assert 0 < summary.confidence <= 1
    assert summary.market_data["price"]["price"] == 450.0
    assert len(summary.analyst_reports) == 3
    assert summary.key_findings
    assert "Valuation is elevated." in summary.risks
    assert summary.next_steps


def test_supervisor_surfaces_conflicting_reports() -> None:
    supervisor = SupervisorAgent(
        market_data_agent=StubMarketDataAgent(),  # type: ignore[arg-type]
        fundamental_agent=StubAnalystAgent(
            AnalystReport(
                symbol="MSFT",
                agent_type="fundamental",
                rating="bullish",
                confidence=0.80,
                summary="Fundamentals are strong.",
            )
        ),  # type: ignore[arg-type]
        technical_agent=StubAnalystAgent(
            AnalystReport(
                symbol="MSFT",
                agent_type="technical",
                rating="bearish",
                confidence=0.75,
                summary="Momentum has deteriorated.",
            )
        ),  # type: ignore[arg-type]
        sentiment_agent=StubAnalystAgent(
            AnalystReport(
                symbol="MSFT",
                agent_type="sentiment",
                rating="neutral",
                confidence=0.45,
                summary="Headlines are mixed.",
            )
        ),  # type: ignore[arg-type]
    )

    summary = supervisor.analyze_symbol("MSFT")

    assert summary.conclusion == "neutral"
    assert summary.conflicts
    assert "Bullish and bearish" in summary.conflicts[0]
