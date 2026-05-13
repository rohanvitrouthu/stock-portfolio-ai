from __future__ import annotations

import os
from dataclasses import dataclass, replace

from dotenv import load_dotenv


load_dotenv()


@dataclass(frozen=True)
class Settings:
    openrouter_api_key: str = ""
    openrouter_base_url: str = "https://openrouter.ai/api/v1"
    openrouter_model: str = "openrouter/auto"

    @classmethod
    def from_env(cls) -> "Settings":
        return cls(
            openrouter_api_key=os.getenv("OPENROUTER_API_KEY", ""),
            openrouter_base_url=os.getenv(
                "OPENROUTER_BASE_URL", "https://openrouter.ai/api/v1"
            ),
            openrouter_model=os.getenv("OPENROUTER_MODEL", "openrouter/auto"),
        )

    def with_model(self, model: str) -> "Settings":
        return replace(self, openrouter_model=model)


def load_settings(*, model: str | None = None) -> Settings:
    settings = Settings.from_env()
    if model:
        return settings.with_model(model)
    return settings
