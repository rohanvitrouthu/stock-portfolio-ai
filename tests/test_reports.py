from __future__ import annotations

from pydantic import ValidationError

from stock_portfolio_ai.reports import AnalystReport, EvidenceItem


def test_analyst_report_serializes_to_json() -> None:
    report = AnalystReport(
        symbol="MSFT",
        agent_type="fundamental",
        rating="bullish",
        confidence=0.72,
        summary="Fundamentals are supported by profitability and cash-flow evidence.",
        key_points=["Free cash flow is positive."],
        risks=["Valuation should be monitored."],
        evidence=[
            EvidenceItem(
                label="Free cash flow",
                value=72_000_000_000,
                source="yfinance.cashflow",
            )
        ],
    )

    payload = report.model_dump()
    assert payload["symbol"] == "MSFT"
    assert payload["agent_type"] == "fundamental"
    assert payload["evidence"][0]["source"] == "yfinance.cashflow"
    assert report.model_dump_json()


def test_analyst_report_rejects_invalid_confidence() -> None:
    try:
        AnalystReport(
            symbol="MSFT",
            agent_type="fundamental",
            rating="bullish",
            confidence=1.5,
            summary="Invalid confidence.",
        )
    except ValidationError as error:
        assert "confidence" in str(error)
    else:
        raise AssertionError("Expected confidence validation to fail.")
