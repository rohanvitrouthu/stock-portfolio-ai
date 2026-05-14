from __future__ import annotations

from collections.abc import Callable
from datetime import datetime
import re
from typing import Any

import yfinance as yf
from langchain.tools import tool
from langchain_openai import ChatOpenAI
from langgraph.prebuilt import create_react_agent

from stock_portfolio_ai.config import Settings, load_settings
from stock_portfolio_ai.reports import AnalystReport, EvidenceItem, Rating


POSITIVE_TERMS = {
    "accelerate",
    "approval",
    "beat",
    "beats",
    "boost",
    "bullish",
    "buyback",
    "growth",
    "higher",
    "improve",
    "improved",
    "improves",
    "launch",
    "outperform",
    "profit",
    "profits",
    "raise",
    "raised",
    "raises",
    "rally",
    "rallies",
    "record",
    "recover",
    "rise",
    "rises",
    "strong",
    "surge",
    "upgrade",
    "upgraded",
    "win",
}

NEGATIVE_TERMS = {
    "bearish",
    "cut",
    "cuts",
    "decline",
    "declines",
    "delay",
    "downgrade",
    "downgraded",
    "drop",
    "falls",
    "fraud",
    "lawsuit",
    "loss",
    "losses",
    "miss",
    "misses",
    "probe",
    "recall",
    "risk",
    "slowing",
    "slumps",
    "weak",
    "weaker",
    "warning",
}


def _normalize_symbol(symbol: str) -> str:
    normalized = symbol.strip().upper()
    if not normalized:
        raise ValueError("A non-empty stock symbol is required.")
    return normalized


def _error_result(symbol: str, error: Exception) -> dict[str, Any]:
    return {"symbol": symbol, "error": str(error)}


def _parse_news_timestamp(raw_value: Any) -> str | None:
    if raw_value is None:
        return None
    if isinstance(raw_value, (int, float)):
        return datetime.fromtimestamp(raw_value).isoformat()
    return str(raw_value)


def _extract_headline(item: dict[str, Any]) -> dict[str, Any] | None:
    if not isinstance(item, dict):
        return None

    content = item.get("content", {})
    title = item.get("title") or content.get("title")
    if not title:
        return None

    return {
        "title": title,
        "publisher": item.get("publisher") or content.get("provider", {}).get("displayName"),
        "link": item.get("link") or content.get("canonicalUrl", {}).get("url"),
        "published_at": _parse_news_timestamp(
            item.get("providerPublishTime") or content.get("pubDate")
        ),
    }


def _tokenize(text: str) -> list[str]:
    return re.findall(r"[a-z]+", text.lower())


def _score_headline(title: str) -> dict[str, Any]:
    tokens = _tokenize(title)
    positive_matches = sorted({token for token in tokens if token in POSITIVE_TERMS})
    negative_matches = sorted({token for token in tokens if token in NEGATIVE_TERMS})
    raw_score = len(positive_matches) - len(negative_matches)

    if raw_score > 0:
        sentiment = "positive"
    elif raw_score < 0:
        sentiment = "negative"
    else:
        sentiment = "neutral"

    return {
        "sentiment": sentiment,
        "score": raw_score,
        "positive_terms": positive_matches,
        "negative_terms": negative_matches,
    }


def _interpret_sentiment(aggregate: dict[str, Any]) -> list[str]:
    headline_count = int(aggregate.get("headline_count", 0))
    average_score = float(aggregate.get("average_score", 0.0))
    positive_count = int(aggregate.get("positive_count", 0))
    negative_count = int(aggregate.get("negative_count", 0))
    neutral_count = int(aggregate.get("neutral_count", 0))

    interpretation = [
        (
            f"Recent news sentiment is based on {headline_count} headline"
            f"{'' if headline_count == 1 else 's'}."
        ),
        (
            f"Headline mix is {positive_count} positive, {negative_count} negative, "
            f"and {neutral_count} neutral."
        ),
    ]

    if average_score >= 0.35:
        interpretation.append("Average headline tone is positive, suggesting supportive near-term news flow.")
    elif average_score <= -0.35:
        interpretation.append("Average headline tone is negative, suggesting near-term news-flow risk.")
    else:
        interpretation.append("Average headline tone is mixed or neutral.")

    return interpretation


def _choose_sentiment_rating(aggregate: dict[str, Any]) -> Rating:
    average_score = float(aggregate.get("average_score", 0.0))
    negative_count = int(aggregate.get("negative_count", 0))
    positive_count = int(aggregate.get("positive_count", 0))

    if average_score >= 0.35 and positive_count >= negative_count:
        return "bullish"
    if average_score <= -0.35 and negative_count >= positive_count:
        return "bearish"
    return "neutral"


def _build_sentiment_evidence(analysis: dict[str, Any]) -> list[EvidenceItem]:
    aggregate = analysis.get("aggregate", {})
    evidence = [
        EvidenceItem(
            label="Average headline sentiment score",
            value=aggregate.get("average_score"),
            source="yfinance.news",
            explanation="Positive terms minus negative terms, averaged across recent headlines.",
        ),
        EvidenceItem(
            label="Analyzed headline count",
            value=aggregate.get("headline_count"),
            source="yfinance.news",
        ),
    ]

    for headline in analysis.get("headlines", [])[:5]:
        score = headline.get("sentiment_score")
        title = headline.get("title")
        if title:
            evidence.append(
                EvidenceItem(
                    label="Headline sentiment",
                    value=score,
                    source="yfinance.news",
                    explanation=title,
                )
            )

    return evidence


def _build_sentiment_report(analysis: dict[str, Any]) -> AnalystReport:
    symbol = _normalize_symbol(analysis["symbol"])
    aggregate = analysis.get("aggregate", {})
    headline_count = int(aggregate.get("headline_count", 0))

    if headline_count == 0:
        return AnalystReport(
            symbol=symbol,
            agent_type="sentiment",
            rating="insufficient_data",
            confidence=0.0,
            summary=f"Sentiment analysis for {symbol} has insufficient recent news.",
            risks=["Required headline evidence was unavailable."],
        )

    rating = _choose_sentiment_rating(aggregate)
    confidence = min(0.75, 0.30 + (headline_count * 0.04))
    key_points = list(analysis.get("interpretation", []))
    risks = [
        point
        for point in key_points
        if any(term in point.lower() for term in ("negative", "risk", "mixed"))
    ]

    return AnalystReport(
        symbol=symbol,
        agent_type="sentiment",
        rating=rating,
        confidence=confidence,
        summary=(
            f"Sentiment analysis for {symbol} is {rating.replace('_', ' ')} "
            "based on recent headline tone and news-flow balance."
        ),
        key_points=key_points,
        risks=risks,
        evidence=_build_sentiment_evidence(analysis),
    )


@tool
def get_news_sentiment(symbol: str, limit: int = 10) -> dict[str, Any]:
    """Return a simple sentiment summary for recent company news headlines."""
    normalized_symbol = symbol.strip().upper()

    try:
        normalized_symbol = _normalize_symbol(symbol)
        if limit <= 0:
            raise ValueError("News sentiment limit must be greater than zero.")

        news_items = yf.Ticker(normalized_symbol).news or []

        headlines = []
        for item in news_items[:limit]:
            headline = _extract_headline(item)
            if headline is None:
                continue
            score = _score_headline(headline["title"])
            headline.update(
                {
                    "sentiment": score["sentiment"],
                    "sentiment_score": score["score"],
                    "positive_terms": score["positive_terms"],
                    "negative_terms": score["negative_terms"],
                }
            )
            headlines.append(headline)

        if not headlines:
            raise ValueError(f"No news headlines found for symbol '{normalized_symbol}'.")

        total_score = sum(float(headline["sentiment_score"]) for headline in headlines)
        aggregate = {
            "headline_count": len(headlines),
            "positive_count": sum(1 for headline in headlines if headline["sentiment"] == "positive"),
            "negative_count": sum(1 for headline in headlines if headline["sentiment"] == "negative"),
            "neutral_count": sum(1 for headline in headlines if headline["sentiment"] == "neutral"),
            "average_score": total_score / len(headlines),
        }

        return {
            "symbol": normalized_symbol,
            "headlines": headlines,
            "aggregate": aggregate,
            "interpretation": _interpret_sentiment(aggregate),
        }
    except Exception as error:
        return _error_result(normalized_symbol, error)


class SentimentAnalystAgent:
    """LLM-backed sentiment analyst agent that can score recent yfinance news headlines."""

    def __init__(self, settings: Settings | None = None, *, model: str | None = None) -> None:
        self.settings = settings or load_settings(model=model)
        self.tools = [get_news_sentiment]
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

    def analyze_symbol(self, symbol: str, *, limit: int = 10) -> dict[str, Any]:
        sentiment = self.invoke_tool("get_news_sentiment", symbol=symbol, limit=limit)
        if isinstance(sentiment, dict) and "error" in sentiment:
            return {"symbol": _normalize_symbol(symbol), "error": sentiment["error"]}

        return {
            "symbol": _normalize_symbol(symbol),
            "news_sentiment": sentiment,
            "headlines": sentiment.get("headlines", []),
            "aggregate": sentiment.get("aggregate", {}),
            "interpretation": sentiment.get("interpretation", []),
        }

    def analyze_symbol_report(self, symbol: str, *, limit: int = 10) -> AnalystReport:
        analysis = self.analyze_symbol(symbol, limit=limit)
        if "error" in analysis:
            return AnalystReport(
                symbol=_normalize_symbol(symbol),
                agent_type="sentiment",
                rating="insufficient_data",
                confidence=0.0,
                summary=f"Sentiment analysis for {_normalize_symbol(symbol)} could not be completed.",
                risks=[analysis["error"]],
            )

        return _build_sentiment_report(analysis)

    def run(self, query: str) -> dict[str, Any]:
        if self.agent is None:
            return {
                "error": (
                    "LLM is not configured. Set OPENROUTER_API_KEY to enable SentimentAnalystAgent.run()."
                ),
                "query": query,
            }

        try:
            result = self.agent.invoke({"messages": [("user", query)]})
        except Exception as error:
            return {"error": str(error), "query": query}

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
    "SentimentAnalystAgent",
    "get_news_sentiment",
]
