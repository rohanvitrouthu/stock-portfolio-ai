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


def _calculate_rsi(close: pd.Series, window: int = 14) -> float | None:
    if len(close) <= window:
        return None

    delta = close.diff()
    gain = delta.clip(lower=0).rolling(window).mean()
    loss = (-delta.clip(upper=0)).rolling(window).mean()
    latest_loss = _coerce_number(loss.iloc[-1])
    latest_gain = _coerce_number(gain.iloc[-1])

    if latest_gain is None or latest_loss is None:
        return None
    if latest_loss == 0:
        return 100.0

    relative_strength = latest_gain / latest_loss
    return 100 - (100 / (1 + relative_strength))


def _pct_change(current: float | None, prior: float | None) -> float | None:
    if current is None or prior is None or prior == 0:
        return None
    return ((current - prior) / abs(prior)) * 100


def _interpret_technicals(indicators: dict[str, Any]) -> list[str]:
    interpretation: list[str] = []
    latest_close = _coerce_number(indicators.get("latest_close"))
    sma_20 = _coerce_number(indicators.get("sma_20"))
    sma_50 = _coerce_number(indicators.get("sma_50"))
    rsi_14 = _coerce_number(indicators.get("rsi_14"))
    return_1m = _coerce_number(indicators.get("return_1m_pct"))

    if latest_close is not None and sma_20 is not None:
        if latest_close > sma_20:
            interpretation.append("Price is above the 20-day moving average, indicating positive short-term momentum.")
        else:
            interpretation.append("Price is below the 20-day moving average, indicating weaker short-term momentum.")

    if sma_20 is not None and sma_50 is not None:
        if sma_20 > sma_50:
            interpretation.append("The 20-day moving average is above the 50-day moving average, which supports an upward trend bias.")
        else:
            interpretation.append("The 20-day moving average is below the 50-day moving average, which points to a weaker trend bias.")

    if rsi_14 is not None:
        if rsi_14 >= 70:
            interpretation.append(f"RSI of {rsi_14:.1f} is elevated, suggesting the stock may be overbought in the short term.")
        elif rsi_14 <= 30:
            interpretation.append(f"RSI of {rsi_14:.1f} is low, suggesting the stock may be oversold in the short term.")
        else:
            interpretation.append(f"RSI of {rsi_14:.1f} is in a neutral range.")

    if return_1m is not None:
        direction = "gained" if return_1m >= 0 else "lost"
        interpretation.append(f"The stock has {direction} {abs(return_1m):.1f}% over the last month.")

    return interpretation


def _choose_technical_rating(indicators: dict[str, Any]) -> Rating:
    latest_close = _coerce_number(indicators.get("latest_close"))
    sma_20 = _coerce_number(indicators.get("sma_20"))
    sma_50 = _coerce_number(indicators.get("sma_50"))
    rsi_14 = _coerce_number(indicators.get("rsi_14"))
    return_1m = _coerce_number(indicators.get("return_1m_pct"))
    score = 0

    if latest_close is not None and sma_20 is not None:
        score += 1 if latest_close > sma_20 else -1

    if sma_20 is not None and sma_50 is not None:
        score += 1 if sma_20 > sma_50 else -1

    if return_1m is not None:
        if return_1m > 3:
            score += 1
        elif return_1m < -3:
            score -= 1

    if rsi_14 is not None:
        if rsi_14 >= 75:
            score -= 1
        elif 45 <= rsi_14 <= 65:
            score += 1
        elif rsi_14 <= 25:
            score -= 1

    if score >= 2:
        return "bullish"
    if score <= -2:
        return "bearish"
    return "neutral"


def _build_technical_evidence(indicators: dict[str, Any]) -> list[EvidenceItem]:
    evidence: list[EvidenceItem] = []
    for label, key in (
        ("Latest close", "latest_close"),
        ("20-day moving average", "sma_20"),
        ("50-day moving average", "sma_50"),
        ("14-day RSI", "rsi_14"),
        ("1-month return percent", "return_1m_pct"),
        ("3-month return percent", "return_3m_pct"),
        ("Annualized volatility percent", "annualized_volatility_pct"),
        ("Latest volume", "latest_volume"),
        ("20-day average volume", "avg_volume_20"),
    ):
        value = _coerce_number(indicators.get(key))
        if value is not None:
            evidence.append(EvidenceItem(label=label, value=value, source="yfinance.history"))

    return evidence


def _build_technical_report(analysis: dict[str, Any]) -> AnalystReport:
    symbol = _normalize_symbol(analysis["symbol"])
    indicators = analysis.get("indicators", {})
    key_points = list(analysis.get("interpretation", []))
    evidence = _build_technical_evidence(indicators)

    if not evidence:
        return AnalystReport(
            symbol=symbol,
            agent_type="technical",
            rating="insufficient_data",
            confidence=0.0,
            summary=f"Technical analysis for {symbol} has insufficient price history.",
            risks=["Required price-history evidence was unavailable."],
        )

    rating = _choose_technical_rating(indicators)
    confidence = min(0.80, 0.30 + (len(evidence) * 0.06))
    risks = [
        point
        for point in key_points
        if any(term in point.lower() for term in ("overbought", "below", "weaker", "lost"))
    ]

    return AnalystReport(
        symbol=symbol,
        agent_type="technical",
        rating=rating,
        confidence=confidence,
        summary=(
            f"Technical analysis for {symbol} is {rating.replace('_', ' ')} "
            "based on trend, momentum, return, volume, and volatility evidence."
        ),
        key_points=key_points,
        risks=risks,
        evidence=evidence,
    )


@tool
def get_technical_indicators(symbol: str, period: str = "6mo") -> dict[str, Any]:
    """Return trend, momentum, return, volume, and volatility indicators for a ticker symbol."""
    normalized_symbol = symbol.strip().upper()

    try:
        normalized_symbol = _normalize_symbol(symbol)
        history = yf.Ticker(normalized_symbol).history(period=period, interval="1d", auto_adjust=False)

        if history.empty or "Close" not in history:
            raise ValueError(f"No price history found for symbol '{normalized_symbol}'.")

        cleaned = history.dropna(subset=["Close"]).copy()
        if len(cleaned) < 50:
            raise ValueError(
                f"Not enough price history found for symbol '{normalized_symbol}' to compute technical indicators."
            )

        close = cleaned["Close"]
        volume = cleaned["Volume"] if "Volume" in cleaned else pd.Series(dtype=float)
        latest_close = _coerce_number(close.iloc[-1])
        close_1m_ago = _coerce_number(close.iloc[-22]) if len(close) >= 22 else None
        close_3m_ago = _coerce_number(close.iloc[-66]) if len(close) >= 66 else None
        daily_returns = close.pct_change().dropna()

        indicators = {
            "latest_close": latest_close,
            "sma_20": _coerce_number(close.rolling(20).mean().iloc[-1]),
            "sma_50": _coerce_number(close.rolling(50).mean().iloc[-1]),
            "rsi_14": _calculate_rsi(close),
            "return_1m_pct": _pct_change(latest_close, close_1m_ago),
            "return_3m_pct": _pct_change(latest_close, close_3m_ago),
            "annualized_volatility_pct": _coerce_number(daily_returns.std() * (252**0.5) * 100),
            "latest_volume": _coerce_number(volume.iloc[-1]) if not volume.empty else None,
            "avg_volume_20": _coerce_number(volume.rolling(20).mean().iloc[-1]) if not volume.empty else None,
        }

        return {
            "symbol": normalized_symbol,
            "period": period,
            "indicators": indicators,
            "interpretation": _interpret_technicals(indicators),
        }
    except Exception as error:
        return _error_result(normalized_symbol, error)


class TechnicalAnalystAgent:
    """LLM-backed technical analyst agent that can use yfinance price-history tools."""

    def __init__(self, settings: Settings | None = None, *, model: str | None = None) -> None:
        self.settings = settings or load_settings(model=model)
        self.tools = [get_technical_indicators]
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

    def analyze_symbol(self, symbol: str, *, period: str = "6mo") -> dict[str, Any]:
        indicators = self.invoke_tool("get_technical_indicators", symbol=symbol, period=period)
        if isinstance(indicators, dict) and "error" in indicators:
            return {"symbol": _normalize_symbol(symbol), "error": indicators["error"]}

        return {
            "symbol": _normalize_symbol(symbol),
            "technical_indicators": indicators,
            "indicators": indicators.get("indicators", {}),
            "interpretation": indicators.get("interpretation", []),
        }

    def analyze_symbol_report(self, symbol: str, *, period: str = "6mo") -> AnalystReport:
        analysis = self.analyze_symbol(symbol, period=period)
        if "error" in analysis:
            return AnalystReport(
                symbol=_normalize_symbol(symbol),
                agent_type="technical",
                rating="insufficient_data",
                confidence=0.0,
                summary=f"Technical analysis for {_normalize_symbol(symbol)} could not be completed.",
                risks=[analysis["error"]],
            )

        return _build_technical_report(analysis)

    def run(self, query: str) -> dict[str, Any]:
        if self.agent is None:
            return {
                "error": (
                    "LLM is not configured. Set OPENROUTER_API_KEY to enable TechnicalAnalystAgent.run()."
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
    "TechnicalAnalystAgent",
    "get_technical_indicators",
]
