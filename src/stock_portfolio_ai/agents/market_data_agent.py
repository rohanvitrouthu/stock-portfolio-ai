from __future__ import annotations

from collections.abc import Callable
from datetime import datetime
from typing import Any

import yfinance as yf
from langchain.tools import tool
from langchain_openai import ChatOpenAI
from langgraph.prebuilt import create_react_agent

from stock_portfolio_ai.config import Settings, load_settings


def _normalize_symbol(symbol: str) -> str:
    normalized = symbol.strip().upper()
    if not normalized:
        raise ValueError("A non-empty stock symbol is required.")
    return normalized


def _error_result(symbol: str, error: Exception) -> dict[str, Any]:
    return {"symbol": symbol, "error": str(error)}


@tool
def get_stock_price(symbol: str) -> dict[str, Any]:
    """Return the latest available stock price for a ticker symbol."""
    normalized_symbol = symbol.strip().upper()

    try:
        normalized_symbol = _normalize_symbol(symbol)
        ticker = yf.Ticker(normalized_symbol)
        fast_info = ticker.fast_info or {}
        history = ticker.history(period="5d", interval="1d", auto_adjust=False)

        if history.empty:
            raise ValueError(f"No market data found for symbol '{normalized_symbol}'.")

        price = (
            fast_info.get("lastPrice")
            or fast_info.get("last_price")
            or fast_info.get("regularMarketPrice")
        )
        if price is None:
            price = float(history["Close"].dropna().iloc[-1])

        latest_row = history.dropna(subset=["Close"]).iloc[-1]
        timestamp = latest_row.name

        return {
            "symbol": normalized_symbol,
            "price": float(price),
            "currency": fast_info.get("currency"),
            "exchange": fast_info.get("exchange"),
            "as_of": timestamp.isoformat() if hasattr(timestamp, "isoformat") else str(timestamp),
            "open": float(latest_row["Open"]),
            "high": float(latest_row["High"]),
            "low": float(latest_row["Low"]),
            "close": float(latest_row["Close"]),
            "volume": int(latest_row["Volume"]),
        }
    except Exception as error:
        return _error_result(normalized_symbol, error)


@tool
def get_historical_data(symbol: str, period: str = "1mo") -> dict[str, Any]:
    """Return historical OHLCV candles for a ticker symbol and period."""
    normalized_symbol = symbol.strip().upper()

    try:
        normalized_symbol = _normalize_symbol(symbol)
        ticker = yf.Ticker(normalized_symbol)
        history = ticker.history(period=period, interval="1d", auto_adjust=False)

        if history.empty:
            raise ValueError(
                f"No historical data found for symbol '{normalized_symbol}' with period '{period}'."
            )

        candles: list[dict[str, Any]] = []
        for timestamp, row in history.iterrows():
            candles.append(
                {
                    "date": timestamp.isoformat() if hasattr(timestamp, "isoformat") else str(timestamp),
                    "open": float(row["Open"]),
                    "high": float(row["High"]),
                    "low": float(row["Low"]),
                    "close": float(row["Close"]),
                    "volume": int(row["Volume"]),
                }
            )

        return {
            "symbol": normalized_symbol,
            "period": period,
            "candles": candles,
        }
    except Exception as error:
        return _error_result(normalized_symbol, error)


def _parse_news_timestamp(raw_value: Any) -> str | None:
    if raw_value is None:
        return None
    if isinstance(raw_value, (int, float)):
        return datetime.fromtimestamp(raw_value).isoformat()
    return str(raw_value)


@tool
def get_company_news(symbol: str) -> dict[str, Any]:
    """Return recent company news headlines for a ticker symbol."""
    normalized_symbol = symbol.strip().upper()

    try:
        normalized_symbol = _normalize_symbol(symbol)
        ticker = yf.Ticker(normalized_symbol)
        news_items = ticker.news or []

        if not news_items:
            raise ValueError(f"No news found for symbol '{normalized_symbol}'.")

        headlines: list[dict[str, Any]] = []
        for item in news_items[:10]:
            content = item.get("content", {}) if isinstance(item, dict) else {}
            title = item.get("title") or content.get("title")
            if not title:
                continue

            headlines.append(
                {
                    "title": title,
                    "publisher": item.get("publisher") or content.get("provider", {}).get("displayName"),
                    "link": item.get("link") or content.get("canonicalUrl", {}).get("url"),
                    "published_at": _parse_news_timestamp(
                        item.get("providerPublishTime") or content.get("pubDate")
                    ),
                }
            )

        if not headlines:
            raise ValueError(f"News data for symbol '{normalized_symbol}' did not include headlines.")

        return {"symbol": normalized_symbol, "headlines": headlines}
    except Exception as error:
        return _error_result(normalized_symbol, error)


class MarketDataAgent:
    """LLM-backed market data agent that can use yfinance tools."""

    def __init__(self, settings: Settings | None = None, *, model: str | None = None) -> None:
        self.settings = settings or load_settings(model=model)
        self.tools = [get_stock_price, get_historical_data, get_company_news]
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

    def run(self, query: str) -> dict[str, Any]:
        if self.agent is None:
            return {
                "error": (
                    "LLM is not configured. Set OPENROUTER_API_KEY to enable MarketDataAgent.run()."
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
    "MarketDataAgent",
    "get_stock_price",
    "get_historical_data",
    "get_company_news",
]
