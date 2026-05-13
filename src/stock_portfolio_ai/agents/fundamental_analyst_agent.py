from __future__ import annotations

from collections.abc import Callable
from typing import Any

import pandas as pd
import yfinance as yf
from langchain.tools import tool
from langchain_openai import ChatOpenAI
from langgraph.prebuilt import create_react_agent

from stock_portfolio_ai.config import Settings, load_settings
from stock_portfolio_ai.reports import AnalystReport, EvidenceItem, Rating


def _normalize_symbol(symbol: str) -> str:
    normalized = symbol.strip().upper()
    if not normalized:
        raise ValueError("A non-empty stock symbol is required.")
    return normalized


def _error_result(symbol: str, error: Exception) -> dict[str, Any]:
    return {"symbol": symbol, "error": str(error)}


def _coerce_number(value: Any) -> float | None:
    if value is None:
        return None

    try:
        if pd.isna(value):
            return None
    except TypeError:
        pass

    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _format_period_label(period: Any) -> str:
    if hasattr(period, "date"):
        return str(period.date())
    return str(period)


def _extract_statement_summary(
    frame: pd.DataFrame,
    row_map: dict[str, list[str]],
    *,
    periods: int = 3,
) -> list[dict[str, Any]]:
    if frame.empty:
        return []

    cleaned = frame.dropna(axis=1, how="all")
    if cleaned.empty:
        return []

    summaries: list[dict[str, Any]] = []
    for column in list(cleaned.columns)[:periods]:
        period_summary: dict[str, Any] = {"period": _format_period_label(column)}
        for label, candidates in row_map.items():
            value = None
            for candidate in candidates:
                if candidate in cleaned.index:
                    value = _coerce_number(cleaned.at[candidate, column])
                    if value is not None:
                        break
            period_summary[label] = value
        summaries.append(period_summary)

    return summaries


def _interpret_pe_ratio(pe_ratio: float | None, forward_pe: float | None) -> list[str]:
    commentary: list[str] = []

    if pe_ratio is None:
        commentary.append("Trailing P/E is unavailable, so earnings-based valuation is inconclusive.")
    elif pe_ratio < 15:
        commentary.append(
            f"P/E ratio of {pe_ratio:.2f} is relatively low and can indicate a cheaper valuation if earnings are durable."
        )
    elif pe_ratio <= 25:
        commentary.append(
            f"P/E ratio of {pe_ratio:.2f} sits in a middle range, which suggests a valuation closer to a typical large-cap profile."
        )
    else:
        commentary.append(
            f"P/E ratio of {pe_ratio:.2f} is elevated and can suggest the stock is priced for strong growth."
        )

    if pe_ratio is not None and forward_pe is not None:
        if forward_pe < pe_ratio:
            commentary.append(
                f"Forward P/E of {forward_pe:.2f} is below trailing P/E, which suggests analysts expect earnings growth."
            )
        elif forward_pe > pe_ratio:
            commentary.append(
                f"Forward P/E of {forward_pe:.2f} is above trailing P/E, which can imply slower earnings expectations."
            )

    return commentary


def _interpret_price_to_book(price_to_book: float | None) -> str:
    if price_to_book is None:
        return "Price-to-book is unavailable, so balance-sheet-based valuation is inconclusive."
    if price_to_book < 1:
        return (
            f"Price-to-book of {price_to_book:.2f} is below 1.0, which can indicate the shares trade below accounting book value."
        )
    if price_to_book <= 3:
        return (
            f"Price-to-book of {price_to_book:.2f} is moderate, suggesting the market is not assigning an extreme premium to net assets."
        )
    return (
        f"Price-to-book of {price_to_book:.2f} is high, which is common for high-quality or asset-light businesses but still implies a premium valuation."
    )


def _interpret_dividend_yield(dividend_yield: float | None) -> str:
    if dividend_yield is None:
        return "Dividend yield is unavailable or the company may not pay a dividend."
    if dividend_yield < 0.01:
        return (
            f"Dividend yield of {dividend_yield:.2%} is minimal, so the investment case is likely driven more by growth than income."
        )
    if dividend_yield <= 0.04:
        return (
            f"Dividend yield of {dividend_yield:.2%} is moderate and can contribute to total return without dominating the thesis."
        )
    return (
        f"Dividend yield of {dividend_yield:.2%} is relatively high, which may appeal to income investors but should be checked for sustainability."
    )


def _summarize_financial_trend(
    current_value: float | None,
    prior_value: float | None,
    label: str,
) -> str | None:
    if current_value is None or prior_value is None:
        return None
    if prior_value == 0:
        return None

    change_pct = ((current_value - prior_value) / abs(prior_value)) * 100
    direction = "improved" if change_pct > 0 else "declined"
    return f"{label} {direction} by {abs(change_pct):.1f}% versus the prior annual period."


def _build_fundamental_interpretation(
    metrics: dict[str, Any],
    financials: dict[str, Any],
    cash_flow: dict[str, Any],
) -> list[str]:
    key_metrics = metrics.get("metrics", {})
    metric_commentary = []
    metric_commentary.extend(
        _interpret_pe_ratio(
            _coerce_number(key_metrics.get("pe_ratio")),
            _coerce_number(key_metrics.get("forward_pe")),
        )
    )
    metric_commentary.append(
        _interpret_price_to_book(_coerce_number(key_metrics.get("price_to_book")))
    )
    metric_commentary.append(
        _interpret_dividend_yield(_coerce_number(key_metrics.get("dividend_yield")))
    )

    trend_commentary: list[str] = []
    income_statement = financials.get("income_statement", [])
    if len(income_statement) >= 2:
        latest = income_statement[0]
        prior = income_statement[1]
        for key, label in (
            ("total_revenue", "Revenue"),
            ("net_income", "Net income"),
        ):
            summary = _summarize_financial_trend(
                _coerce_number(latest.get(key)),
                _coerce_number(prior.get(key)),
                label,
            )
            if summary:
                trend_commentary.append(summary)

    cash_flow_items = cash_flow.get("cash_flow", [])
    if len(cash_flow_items) >= 2:
        latest_cf = cash_flow_items[0]
        prior_cf = cash_flow_items[1]
        free_cash_flow_summary = _summarize_financial_trend(
            _coerce_number(latest_cf.get("free_cash_flow")),
            _coerce_number(prior_cf.get("free_cash_flow")),
            "Free cash flow",
        )
        if free_cash_flow_summary:
            trend_commentary.append(free_cash_flow_summary)

    return metric_commentary + trend_commentary


def _latest_value(items: list[dict[str, Any]], key: str) -> float | None:
    if not items:
        return None
    return _coerce_number(items[0].get(key))


def _choose_fundamental_rating(metrics: dict[str, Any], cash_flow: dict[str, Any]) -> Rating:
    key_metrics = metrics.get("metrics", {})
    score = 0

    pe_ratio = _coerce_number(key_metrics.get("pe_ratio"))
    if pe_ratio is not None:
        if pe_ratio <= 25:
            score += 1
        elif pe_ratio > 40:
            score -= 1

    roe = _coerce_number(key_metrics.get("return_on_equity"))
    if roe is not None:
        if roe >= 0.15:
            score += 1
        elif roe <= 0:
            score -= 1

    profit_margin = _coerce_number(key_metrics.get("profit_margin"))
    if profit_margin is not None:
        if profit_margin >= 0.10:
            score += 1
        elif profit_margin <= 0:
            score -= 1

    latest_free_cash_flow = _latest_value(cash_flow.get("cash_flow", []), "free_cash_flow")
    if latest_free_cash_flow is not None:
        if latest_free_cash_flow > 0:
            score += 1
        else:
            score -= 1

    if score >= 2:
        return "bullish"
    if score <= -2:
        return "bearish"
    return "neutral"


def _build_fundamental_evidence(
    metrics: dict[str, Any],
    financials: dict[str, Any],
    cash_flow: dict[str, Any],
) -> list[EvidenceItem]:
    key_metrics = metrics.get("metrics", {})
    evidence: list[EvidenceItem] = []

    for label, key in (
        ("Trailing P/E", "pe_ratio"),
        ("Forward P/E", "forward_pe"),
        ("Price to book", "price_to_book"),
        ("Dividend yield", "dividend_yield"),
        ("Return on equity", "return_on_equity"),
        ("Profit margin", "profit_margin"),
    ):
        value = _coerce_number(key_metrics.get(key))
        if value is not None:
            evidence.append(
                EvidenceItem(
                    label=label,
                    value=value,
                    source="yfinance.info",
                )
            )

    latest_income = financials.get("income_statement", [])
    for label, key in (
        ("Latest revenue", "total_revenue"),
        ("Latest net income", "net_income"),
    ):
        value = _latest_value(latest_income, key)
        if value is not None:
            evidence.append(
                EvidenceItem(
                    label=label,
                    value=value,
                    source="yfinance.income_stmt",
                )
            )

    latest_cash_flow = cash_flow.get("cash_flow", [])
    value = _latest_value(latest_cash_flow, "free_cash_flow")
    if value is not None:
        evidence.append(
            EvidenceItem(
                label="Latest free cash flow",
                value=value,
                source="yfinance.cashflow",
            )
        )

    return evidence


def _build_fundamental_report(analysis: dict[str, Any]) -> AnalystReport:
    symbol = _normalize_symbol(analysis["symbol"])
    metrics = analysis.get("key_metrics", {})
    financials = analysis.get("financials", {})
    cash_flow = analysis.get("cash_flow", {})
    key_points = list(analysis.get("interpretation", []))
    evidence = _build_fundamental_evidence(metrics, financials, cash_flow)

    if not evidence:
        return AnalystReport(
            symbol=symbol,
            agent_type="fundamental",
            rating="insufficient_data",
            confidence=0.0,
            summary=f"Fundamental analysis for {symbol} has insufficient source data.",
            risks=["Required fundamental evidence was unavailable."],
        )

    rating = _choose_fundamental_rating(metrics, cash_flow)
    confidence = min(0.85, 0.35 + (len(evidence) * 0.07))
    summary = (
        f"Fundamental analysis for {symbol} is {rating.replace('_', ' ')} "
        f"based on valuation, profitability, financial trend, and cash-flow evidence."
    )

    risks = [
        point
        for point in key_points
        if any(term in point.lower() for term in ("unavailable", "elevated", "negative", "weak"))
    ]

    return AnalystReport(
        symbol=symbol,
        agent_type="fundamental",
        rating=rating,
        confidence=confidence,
        summary=summary,
        key_points=key_points,
        risks=risks,
        evidence=evidence,
    )


@tool
def get_financials(symbol: str) -> dict[str, Any]:
    """Return income statement and balance sheet summaries for a ticker symbol."""
    normalized_symbol = symbol.strip().upper()

    try:
        normalized_symbol = _normalize_symbol(symbol)
        ticker = yf.Ticker(normalized_symbol)

        income_statement = _extract_statement_summary(
            ticker.income_stmt,
            {
                "total_revenue": ["Total Revenue"],
                "gross_profit": ["Gross Profit"],
                "operating_income": ["Operating Income"],
                "net_income": ["Net Income"],
                "diluted_eps": ["Diluted EPS", "Basic EPS"],
            },
        )
        balance_sheet = _extract_statement_summary(
            ticker.balance_sheet,
            {
                "total_assets": ["Total Assets"],
                "total_liabilities": ["Total Liabilities Net Minority Interest", "Total Liabilities"],
                "stockholders_equity": ["Stockholders Equity"],
                "cash_and_equivalents": ["Cash And Cash Equivalents"],
                "total_debt": ["Total Debt"],
            },
        )

        if not income_statement and not balance_sheet:
            raise ValueError(f"No financial statement data found for symbol '{normalized_symbol}'.")

        return {
            "symbol": normalized_symbol,
            "income_statement": income_statement,
            "balance_sheet": balance_sheet,
        }
    except Exception as error:
        return _error_result(normalized_symbol, error)


@tool
def get_key_metrics(symbol: str) -> dict[str, Any]:
    """Return valuation and shareholder-return metrics for a ticker symbol."""
    normalized_symbol = symbol.strip().upper()

    try:
        normalized_symbol = _normalize_symbol(symbol)
        info = yf.Ticker(normalized_symbol).info or {}

        metrics = {
            "pe_ratio": _coerce_number(info.get("trailingPE")),
            "forward_pe": _coerce_number(info.get("forwardPE")),
            "price_to_book": _coerce_number(info.get("priceToBook")),
            "dividend_yield": _coerce_number(info.get("dividendYield")),
            "return_on_equity": _coerce_number(info.get("returnOnEquity")),
            "profit_margin": _coerce_number(info.get("profitMargins")),
        }

        if all(value is None for value in metrics.values()):
            raise ValueError(f"No key metric data found for symbol '{normalized_symbol}'.")

        interpretations = []
        interpretations.extend(_interpret_pe_ratio(metrics["pe_ratio"], metrics["forward_pe"]))
        interpretations.append(_interpret_price_to_book(metrics["price_to_book"]))
        interpretations.append(_interpret_dividend_yield(metrics["dividend_yield"]))

        roe = metrics["return_on_equity"]
        if roe is not None:
            if roe >= 0.15:
                interpretations.append(
                    f"Return on equity of {roe:.2%} indicates strong profitability relative to shareholder capital."
                )
            elif roe > 0:
                interpretations.append(
                    f"Return on equity of {roe:.2%} is positive but not especially high for a quality compounder."
                )
            else:
                interpretations.append(
                    f"Return on equity of {roe:.2%} is weak or negative, which can point to profitability pressure."
                )

        profit_margin = metrics["profit_margin"]
        if profit_margin is not None:
            interpretations.append(
                f"Profit margin of {profit_margin:.2%} provides context on how much of each revenue dollar converts into profit."
            )

        return {
            "symbol": normalized_symbol,
            "metrics": metrics,
            "interpretation": interpretations,
        }
    except Exception as error:
        return _error_result(normalized_symbol, error)


@tool
def get_cash_flow(symbol: str) -> dict[str, Any]:
    """Return cash flow statement summaries for a ticker symbol."""
    normalized_symbol = symbol.strip().upper()

    try:
        normalized_symbol = _normalize_symbol(symbol)
        cash_flow = _extract_statement_summary(
            yf.Ticker(normalized_symbol).cashflow,
            {
                "operating_cash_flow": ["Operating Cash Flow"],
                "capital_expenditure": ["Capital Expenditure"],
                "free_cash_flow": ["Free Cash Flow"],
                "investing_cash_flow": ["Investing Cash Flow"],
                "financing_cash_flow": ["Financing Cash Flow"],
            },
        )

        if not cash_flow:
            raise ValueError(f"No cash flow data found for symbol '{normalized_symbol}'.")

        interpretations: list[str] = []
        latest = cash_flow[0]
        latest_ocf = _coerce_number(latest.get("operating_cash_flow"))
        latest_fcf = _coerce_number(latest.get("free_cash_flow"))
        if latest_ocf is not None and latest_fcf is not None:
            if latest_fcf > 0:
                interpretations.append(
                    "Positive free cash flow suggests the business is generating cash after reinvestment needs."
                )
            else:
                interpretations.append(
                    "Negative free cash flow suggests reinvestment or operating demands are currently consuming cash."
                )

        if len(cash_flow) >= 2:
            change_summary = _summarize_financial_trend(
                _coerce_number(cash_flow[0].get("operating_cash_flow")),
                _coerce_number(cash_flow[1].get("operating_cash_flow")),
                "Operating cash flow",
            )
            if change_summary:
                interpretations.append(change_summary)

        return {
            "symbol": normalized_symbol,
            "cash_flow": cash_flow,
            "interpretation": interpretations,
        }
    except Exception as error:
        return _error_result(normalized_symbol, error)


class FundamentalAnalystAgent:
    """LLM-backed fundamental analyst agent that can use yfinance tools."""

    def __init__(self, settings: Settings | None = None, *, model: str | None = None) -> None:
        self.settings = settings or load_settings(model=model)
        self.tools = [get_financials, get_key_metrics, get_cash_flow]
        self._tool_map: dict[str, Callable[..., Any]] = {
            tool_.name: tool_ for tool_ in self.tools
        }
        self.llm = self._build_llm()
        self.agent = create_react_agent(self.llm, self.tools) if self.llm else None

    def _build_llm(self) -> ChatOpenAI | None:
        if not self.settings.openrouter_api_key:
            return None

        return ChatOpenAI(
            api_key=self.settings.openrouter_api_key,
            base_url=self.settings.openrouter_base_url,
            model=self.settings.openrouter_model,
            temperature=0,
        )

    def invoke_tool(self, tool_name: str, **kwargs: Any) -> Any:
        tool_ = self._tool_map.get(tool_name)
        if tool_ is None:
            available = ", ".join(sorted(self._tool_map))
            raise ValueError(f"Unknown tool '{tool_name}'. Available tools: {available}")
        return tool_.invoke(kwargs)

    def analyze_symbol(self, symbol: str) -> dict[str, Any]:
        financials = self.invoke_tool("get_financials", symbol=symbol)
        metrics = self.invoke_tool("get_key_metrics", symbol=symbol)
        cash_flow = self.invoke_tool("get_cash_flow", symbol=symbol)

        errors = [
            payload["error"]
            for payload in (financials, metrics, cash_flow)
            if isinstance(payload, dict) and "error" in payload
        ]
        if errors:
            return {"symbol": _normalize_symbol(symbol), "error": "; ".join(errors)}

        interpretation = _build_fundamental_interpretation(metrics, financials, cash_flow)

        return {
            "symbol": _normalize_symbol(symbol),
            "financials": financials,
            "key_metrics": metrics,
            "cash_flow": cash_flow,
            "interpretation": interpretation,
        }

    def analyze_symbol_report(self, symbol: str) -> AnalystReport:
        analysis = self.analyze_symbol(symbol)
        if "error" in analysis:
            return AnalystReport(
                symbol=_normalize_symbol(symbol),
                agent_type="fundamental",
                rating="insufficient_data",
                confidence=0.0,
                summary=f"Fundamental analysis for {_normalize_symbol(symbol)} could not be completed.",
                risks=[analysis["error"]],
            )

        return _build_fundamental_report(analysis)

    def run(self, query: str) -> dict[str, Any]:
        if self.agent is None:
            return {
                "error": (
                    "LLM is not configured. Set OPENROUTER_API_KEY to enable FundamentalAnalystAgent.run()."
                ),
                "query": query,
            }

        result = self.agent.invoke({"messages": [("user", query)]})
        messages = result.get("messages", [])
        serialized_messages = [
            {
                "type": message.__class__.__name__,
                "content": getattr(message, "content", ""),
            }
            for message in messages
        ]

        final_output = ""
        if messages:
            final_output = getattr(messages[-1], "content", "") or ""

        return {
            "query": query,
            "final_output": final_output,
            "messages": serialized_messages,
        }


__all__ = [
    "FundamentalAnalystAgent",
    "get_financials",
    "get_key_metrics",
    "get_cash_flow",
]
