from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, Field

AgentType = Literal["fundamental", "technical", "macro", "news"]
Rating = Literal["bullish", "neutral", "bearish", "insufficient_data"]


class EvidenceItem(BaseModel):
    label: str
    source: str
    value: str | float | int | None = None
    explanation: str | None = None


class AnalystReport(BaseModel):
    symbol: str
    agent_type: AgentType
    rating: Rating
    confidence: float = Field(ge=0, le=1)
    summary: str
    key_points: list[str] = Field(default_factory=list)
    risks: list[str] = Field(default_factory=list)
    evidence: list[EvidenceItem] = Field(default_factory=list)


__all__ = [
    "AgentType",
    "AnalystReport",
    "EvidenceItem",
    "Rating",
]
