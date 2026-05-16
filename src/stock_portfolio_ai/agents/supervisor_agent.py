from __future__ import annotations

from typing import Any

from stock_portfolio_ai.agents.fundamental_analyst_agent import FundamentalAnalystAgent
from stock_portfolio_ai.agents.market_data_agent import MarketDataAgent
from stock_portfolio_ai.agents.sentiment_analyst_agent import SentimentAnalystAgent
from stock_portfolio_ai.agents.technical_analyst_agent import TechnicalAnalystAgent
from stock_portfolio_ai.reports import AnalystReport, InvestmentConclusion, InvestmentSummary


def _normalize_symbol(symbol: str) -> str:
    normalized = symbol.strip().upper()
    if not normalized:
        raise ValueError("A non-empty stock symbol is required.")
    return normalized


RATING_SCORES = {
    "bullish": 1.0,
    "neutral": 0.0,
    "bearish": -1.0,
}

AGENT_WEIGHTS = {
    "fundamental": 0.40,
    "technical": 0.30,
    "sentiment": 0.20,
}


class SupervisorAgent:
    """Consolidates market data and analyst reports into one investment summary."""

    def __init__(
        self,
        *,
        model: str | None = None,
        market_data_agent: MarketDataAgent | None = None,
        fundamental_agent: FundamentalAnalystAgent | None = None,
        technical_agent: TechnicalAnalystAgent | None = None,
        sentiment_agent: SentimentAnalystAgent | None = None,
    ) -> None:
        self.model = model
        self.market_data_agent = market_data_agent or MarketDataAgent(model=model)
        self.fundamental_agent = fundamental_agent or FundamentalAnalystAgent(model=model)
        self.technical_agent = technical_agent or TechnicalAnalystAgent(model=model)
        self.sentiment_agent = sentiment_agent or SentimentAnalystAgent(model=model)

    def collect_market_data(self, symbol: str) -> dict[str, Any]:
        normalized_symbol = _normalize_symbol(symbol)
        price = self.market_data_agent.invoke_tool("get_stock_price", symbol=normalized_symbol)
        news = self.market_data_agent.invoke_tool("get_company_news", symbol=normalized_symbol)

        snapshot: dict[str, Any] = {"symbol": normalized_symbol}
        if isinstance(price, dict) and "error" not in price:
            snapshot["price"] = price
        elif isinstance(price, dict):
            snapshot["price_error"] = price["error"]

        if isinstance(news, dict) and "error" not in news:
            snapshot["news"] = news
        elif isinstance(news, dict):
            snapshot["news_error"] = news["error"]

        return snapshot

    def collect_analyst_reports(self, symbol: str) -> list[AnalystReport]:
        normalized_symbol = _normalize_symbol(symbol)
        return [
            self.fundamental_agent.analyze_symbol_report(normalized_symbol),
            self.technical_agent.analyze_symbol_report(normalized_symbol),
            self.sentiment_agent.analyze_symbol_report(normalized_symbol),
        ]

    def analyze_symbol(self, symbol: str) -> InvestmentSummary:
        normalized_symbol = _normalize_symbol(symbol)
        market_data = self.collect_market_data(normalized_symbol)
        analyst_reports = self.collect_analyst_reports(normalized_symbol)

        conclusion = _choose_conclusion(analyst_reports)
        confidence = _calculate_confidence(analyst_reports)
        key_findings = _build_key_findings(market_data, analyst_reports)
        risks = _build_risks(market_data, analyst_reports)
        conflicts = _build_conflicts(analyst_reports)
        next_steps = _build_next_steps(conclusion, conflicts, analyst_reports)

        return InvestmentSummary(
            symbol=normalized_symbol,
            conclusion=conclusion,
            confidence=confidence,
            summary=_build_summary(normalized_symbol, conclusion, confidence, analyst_reports),
            market_data=market_data,
            analyst_reports=analyst_reports,
            key_findings=key_findings,
            risks=risks,
            conflicts=conflicts,
            next_steps=next_steps,
        )


def _weighted_score(reports: list[AnalystReport]) -> tuple[float, float]:
    score = 0.0
    weight_sum = 0.0

    for report in reports:
        rating_score = RATING_SCORES.get(report.rating)
        if rating_score is None:
            continue

        weight = AGENT_WEIGHTS.get(report.agent_type, 0.10)
        effective_weight = weight * max(report.confidence, 0.10)
        score += rating_score * effective_weight
        weight_sum += effective_weight

    if weight_sum == 0:
        return 0.0, 0.0
    return score / weight_sum, weight_sum


def _choose_conclusion(reports: list[AnalystReport]) -> InvestmentConclusion:
    score, weight_sum = _weighted_score(reports)
    if weight_sum == 0:
        return "insufficient_data"
    if score >= 0.35:
        return "bullish"
    if score <= -0.35:
        return "bearish"
    return "neutral"


def _calculate_confidence(reports: list[AnalystReport]) -> float:
    usable_reports = [report for report in reports if report.rating != "insufficient_data"]
    if not usable_reports:
        return 0.0

    average_confidence = sum(report.confidence for report in usable_reports) / len(usable_reports)
    coverage = len(usable_reports) / len(reports)
    conflict_penalty = 0.15 if _has_bull_bear_conflict(usable_reports) else 0.0
    return max(0.0, min(0.90, (average_confidence * coverage) - conflict_penalty))


def _build_key_findings(
    market_data: dict[str, Any],
    reports: list[AnalystReport],
) -> list[str]:
    findings: list[str] = []
    price = market_data.get("price", {})
    if price:
        current_price = price.get("price")
        currency = price.get("currency") or ""
        exchange = price.get("exchange") or "unknown exchange"
        findings.append(f"Market data shows a latest price of {current_price} {currency} on {exchange}.")

    news = market_data.get("news", {})
    headlines = news.get("headlines", []) if isinstance(news, dict) else []
    if headlines:
        findings.append(f"Market data includes {len(headlines)} recent company headline(s).")

    for report in reports:
        findings.append(
            f"{report.agent_type.title()} view is {report.rating.replace('_', ' ')} "
            f"with {report.confidence:.0%} confidence: {report.summary}"
        )

    return findings


def _build_risks(
    market_data: dict[str, Any],
    reports: list[AnalystReport],
) -> list[str]:
    risks: list[str] = []
    if market_data.get("price_error"):
        risks.append(f"Market price unavailable: {market_data['price_error']}")
    if market_data.get("news_error"):
        risks.append(f"Recent news unavailable: {market_data['news_error']}")

    for report in reports:
        if report.rating == "bearish":
            risks.append(f"{report.agent_type.title()} report is bearish.")
        if report.rating == "insufficient_data":
            risks.append(f"{report.agent_type.title()} report has insufficient data.")
        risks.extend(report.risks)

    return _dedupe(risks)


def _build_conflicts(reports: list[AnalystReport]) -> list[str]:
    conflicts: list[str] = []
    bullish = [report.agent_type for report in reports if report.rating == "bullish"]
    bearish = [report.agent_type for report in reports if report.rating == "bearish"]

    if bullish and bearish:
        conflicts.append(
            "Bullish and bearish analyst views disagree: "
            f"bullish={', '.join(bullish)}; bearish={', '.join(bearish)}."
        )

    missing = [report.agent_type for report in reports if report.rating == "insufficient_data"]
    if missing:
        conflicts.append(f"Some analyst views are missing: {', '.join(missing)}.")

    return conflicts


def _build_next_steps(
    conclusion: InvestmentConclusion,
    conflicts: list[str],
    reports: list[AnalystReport],
) -> list[str]:
    next_steps: list[str] = []
    if conclusion == "bullish":
        next_steps.append("Review portfolio fit, valuation sensitivity, and position sizing before acting.")
    elif conclusion == "bearish":
        next_steps.append("Avoid or reduce exposure unless new evidence changes the bearish setup.")
    elif conclusion == "neutral":
        next_steps.append("Keep on watchlist and wait for stronger alignment across analyst signals.")
    else:
        next_steps.append("Collect missing market or analyst evidence before forming a decision.")

    if conflicts:
        next_steps.append("Resolve conflicting signals by inspecting the underlying evidence items.")

    low_confidence = [report.agent_type for report in reports if report.confidence < 0.35]
    if low_confidence:
        next_steps.append(f"Improve low-confidence inputs: {', '.join(low_confidence)}.")

    return next_steps


def _build_summary(
    symbol: str,
    conclusion: InvestmentConclusion,
    confidence: float,
    reports: list[AnalystReport],
) -> str:
    ratings = ", ".join(
        f"{report.agent_type}={report.rating.replace('_', ' ')}" for report in reports
    )
    return (
        f"Supervisor conclusion for {symbol} is {conclusion.replace('_', ' ')} "
        f"with {confidence:.0%} confidence after consolidating {ratings}."
    )


def _has_bull_bear_conflict(reports: list[AnalystReport]) -> bool:
    ratings = {report.rating for report in reports}
    return "bullish" in ratings and "bearish" in ratings


def _dedupe(items: list[str]) -> list[str]:
    seen = set()
    deduped = []
    for item in items:
        if item in seen:
            continue
        seen.add(item)
        deduped.append(item)
    return deduped


__all__ = ["SupervisorAgent"]
