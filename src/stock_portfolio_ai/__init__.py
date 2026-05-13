from .config import Settings, load_settings


def main() -> None:
    settings = load_settings()
    print(
        "stock-portfolio-ai bootstrap ready "
        f"(default model: {settings.openrouter_model})"
    )


__all__ = ["Settings", "load_settings", "main"]
