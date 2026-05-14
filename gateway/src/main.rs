use askama::Template;
use axum::{
    extract::{Form, Path, State},
    http::Request,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use gateway::{IndexComponent, IndexScraper, QuoteResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path as StdPath, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{
    filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt, Layer,
};

#[derive(Clone)]
struct AppState {
    scraper: Arc<IndexScraper>,
    quote_cache: Arc<Mutex<HashMap<String, CachedStockRows>>>,
}

#[derive(Clone)]
struct CachedStockRows {
    stocks: Vec<StockResult>,
    fetched_at: Instant,
}

// --- Templates ---

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {}

#[derive(Template)]
#[template(path = "index_results_fragment.html")]
struct IndexResultsFragment<'a> {
    query: &'a str,
    results: Vec<IndexResult>,
}

#[derive(Template)]
#[template(path = "index_detail.html")]
struct IndexDetailTemplate<'a> {
    index_name: &'a str,
    symbol: &'a str,
    top_stocks: Vec<StockResult>,
}

#[derive(Template)]
#[template(path = "index_detail_fragment.html")]
struct IndexDetailFragment<'a> {
    index_name: &'a str,
    symbol: &'a str,
    top_stocks: Vec<StockResult>,
}

#[derive(Template)]
#[template(path = "stock_search_results_fragment.html")]
struct StockSearchResultsFragment {
    results: Vec<StockResult>,
}

#[derive(Template)]
#[template(path = "stock_rows_fragment.html")]
struct StockRowsFragment {
    stocks: Vec<StockResult>,
}

#[derive(Template)]
#[template(path = "stock_detail.html")]
struct StockDetailTemplate<'a> {
    symbol: &'a str,
    model_id: &'a str,
}

#[derive(Template)]
#[template(path = "stock_detail_fragment.html")]
struct StockDetailFragment<'a> {
    symbol: &'a str,
    model_id: &'a str,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    models: Vec<ModelOptionView>,
}

#[derive(Template)]
#[template(path = "settings_fragment.html")]
struct SettingsFragment {
    models: Vec<ModelOptionView>,
}

#[derive(Template)]
#[template(path = "research_report_fragment.html")]
struct ResearchReportFragment {
    title: String,
    rating: String,
    confidence: String,
    summary: String,
    key_points: Vec<String>,
    risks: Vec<String>,
    evidence: Vec<EvidenceView>,
}

#[derive(Template)]
#[template(path = "research_market_fragment.html")]
struct ResearchMarketFragment {
    symbol: String,
    metrics: Vec<MetricView>,
    headlines: Vec<HeadlineView>,
}

#[derive(Template)]
#[template(path = "research_conclusion_fragment.html")]
struct ResearchConclusionFragment {
    conclusion: String,
    confidence: String,
    summary: String,
    key_findings: Vec<String>,
    risks: Vec<String>,
    conflicts: Vec<String>,
    next_steps: Vec<String>,
}

#[derive(Template)]
#[template(path = "research_error_fragment.html")]
struct ResearchErrorFragment {
    message: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct IndexResult {
    name: String,
    symbol: String,
    country: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct StockResult {
    symbol: String,
    name: String,
    sector: String,
    market_cap: String,
    price: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct ModelConfig {
    default_model: String,
    models: Vec<ModelOption>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ModelOption {
    id: String,
    label: String,
}

struct ModelOptionView {
    id: String,
    label: String,
    selected: bool,
}

struct EvidenceView {
    label: String,
    value: String,
    source: String,
    explanation: String,
}

struct MetricView {
    label: String,
    value: String,
}

struct HeadlineView {
    title: String,
    publisher: String,
    published_at: String,
}

// --- Handlers ---

async fn home_handler() -> impl IntoResponse {
    let template = IndexTemplate {};
    Html(template.render().unwrap())
}

async fn settings_handler(req: Request<axum::body::Body>) -> impl IntoResponse {
    let headers = req.headers();
    let is_htmx = headers.get("hx-request").is_some();
    let config = load_model_config();
    let models = model_option_views(&config);

    if is_htmx {
        Html(SettingsFragment { models }.render().unwrap())
    } else {
        Html(SettingsTemplate { models }.render().unwrap())
    }
    .into_response()
}

#[derive(Deserialize)]
struct SearchQuery {
    index_query: String,
}

async fn search_indices_handler(
    State(state): State<AppState>,
    Form(query): Form<SearchQuery>,
) -> impl IntoResponse {
    let q = query.index_query.to_lowercase();

    let all_indices = vec![
        IndexResult {
            name: "S&P 500".to_string(),
            symbol: "^GSPC".to_string(),
            country: "USA".to_string(),
        },
        IndexResult {
            name: "Nasdaq 100".to_string(),
            symbol: "^NDX".to_string(),
            country: "USA".to_string(),
        },
        IndexResult {
            name: "Nifty 50".to_string(),
            symbol: "^NSEI".to_string(),
            country: "India".to_string(),
        },
        IndexResult {
            name: "FTSE 100".to_string(),
            symbol: "^FTSE".to_string(),
            country: "UK".to_string(),
        },
        IndexResult {
            name: "Nikkei 225".to_string(),
            symbol: "^N225".to_string(),
            country: "Japan".to_string(),
        },
        IndexResult {
            name: "DAX 40".to_string(),
            symbol: "^GDAXI".to_string(),
            country: "Germany".to_string(),
        },
    ];

    let filtered: Vec<IndexResult> = all_indices
        .into_iter()
        .filter(|i| i.name.to_lowercase().contains(&q) || i.symbol.to_lowercase().contains(&q))
        .collect();

    if !q.trim().is_empty() {
        for symbol in filtered.iter().map(|index| index.symbol.clone()) {
            let state = state.clone();
            tokio::spawn(async move {
                if let Err(error) = prefetch_index_data(state, symbol.clone()).await {
                    tracing::debug!("Prefetch failed for {}: {}", symbol, error);
                }
            });
        }
    }

    let template = IndexResultsFragment {
        query: &query.index_query,
        results: filtered,
    };

    Html(template.render().unwrap())
}

async fn index_detail_handler(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    req: Request<axum::body::Body>,
) -> impl IntoResponse {
    let headers = req.headers();
    let is_htmx = headers.get("hx-request").is_some();

    let index_name = match symbol.as_str() {
        "^GSPC" => "S&P 500",
        "^NDX" => "Nasdaq 100",
        "^NSEI" => "Nifty 50",
        "^FTSE" => "FTSE 100",
        "^N225" => "Nikkei 225",
        "^GDAXI" => "DAX 40",
        _ => "Unknown Index",
    };

    let top_stocks = match get_top_stocks(&state.scraper, &symbol).await {
        Ok(stocks) if !stocks.is_empty() => stocks,
        Ok(_) => {
            tracing::warn!(
                "No live stocks found for {}; falling back to mock rows",
                symbol
            );
            mock_top_stocks()
        }
        Err(error) => {
            tracing::warn!("Failed to load live stocks for {}: {}", symbol, error);
            mock_top_stocks()
        }
    };

    if is_htmx {
        let template = IndexDetailFragment {
            index_name,
            symbol: &symbol,
            top_stocks,
        };
        Html(template.render().unwrap())
    } else {
        let template = IndexDetailTemplate {
            index_name,
            symbol: &symbol,
            top_stocks,
        };
        Html(template.render().unwrap())
    }
    .into_response()
}

#[derive(Deserialize)]
struct StockSearchQuery {
    stock_query: String,
}

async fn search_stocks_handler(
    State(state): State<AppState>,
    Path(index_symbol): Path<String>,
    Form(query): Form<StockSearchQuery>,
) -> impl IntoResponse {
    let q = query.stock_query.to_lowercase();

    let all_stocks = match state.scraper.get_index_components(&index_symbol).await {
        Ok(components) => components
            .into_iter()
            .map(stock_from_component)
            .collect::<Vec<_>>(),
        Err(error) => {
            tracing::warn!(
                "Failed to search live components for {}: {}",
                index_symbol,
                error
            );
            mock_top_stocks()
        }
    };

    let filtered: Vec<StockResult> = all_stocks
        .into_iter()
        .filter(|s| s.name.to_lowercase().contains(&q) || s.symbol.to_lowercase().contains(&q))
        .collect();

    let template = StockSearchResultsFragment { results: filtered };

    Html(template.render().unwrap())
}

async fn stock_detail_handler(
    Path(symbol): Path<String>,
    req: Request<axum::body::Body>,
) -> impl IntoResponse {
    let headers = req.headers();
    let is_htmx = headers.get("hx-request").is_some();
    let normalized_symbol = normalize_stock_symbol(&symbol);
    let config = load_model_config();

    if is_htmx {
        Html(
            StockDetailFragment {
                symbol: &normalized_symbol,
                model_id: &config.default_model,
            }
            .render()
            .unwrap(),
        )
    } else {
        Html(
            StockDetailTemplate {
                symbol: &normalized_symbol,
                model_id: &config.default_model,
            }
            .render()
            .unwrap(),
        )
    }
    .into_response()
}

async fn stock_research_handler(
    Path((symbol, section)): Path<(String, String)>,
) -> impl IntoResponse {
    let normalized_symbol = normalize_stock_symbol(&symbol);
    let config = load_model_config();

    match run_stock_research_bridge(&normalized_symbol, &section, &config.default_model).await {
        Ok(payload) => render_research_payload(&section, payload).into_response(),
        Err(error) => Html(
            ResearchErrorFragment {
                message: format!("Research generation failed for {normalized_symbol}: {error}"),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(LevelFilter::INFO))
        .init();

    let state = AppState {
        scraper: Arc::new(IndexScraper::new()),
        quote_cache: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/", get(home_handler))
        .route("/settings", get(settings_handler))
        .route("/search/indices", post(search_indices_handler))
        .route("/index/:symbol/quotes", get(index_quotes_handler))
        .route("/index/:symbol", get(index_detail_handler))
        .route("/search/stocks/:symbol", post(search_stocks_handler))
        .route(
            "/stock/:symbol/research/:section",
            get(stock_research_handler),
        )
        .route("/stock/:symbol", get(stock_detail_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("🚀 Gateway server running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_top_stocks(
    scraper: &IndexScraper,
    index_symbol: &str,
) -> anyhow::Result<Vec<StockResult>> {
    let components = scraper.get_index_components(index_symbol).await?;
    Ok(components
        .into_iter()
        .take(10)
        .map(stock_from_component)
        .collect())
}

async fn prefetch_index_data(state: AppState, index_symbol: String) -> anyhow::Result<()> {
    let components = state.scraper.get_index_components(&index_symbol).await?;
    let _ = get_enriched_stocks(&state, &index_symbol, &components).await?;
    Ok(())
}

async fn index_quotes_handler(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> impl IntoResponse {
    let components = match state.scraper.get_index_components(&symbol).await {
        Ok(components) => components,
        Err(error) => {
            tracing::warn!("Failed to load constituents for quote refresh: {}", error);
            Vec::new()
        }
    };
    let fallback_stocks = components
        .iter()
        .take(10)
        .cloned()
        .map(stock_from_component)
        .collect::<Vec<_>>();

    let stocks = match get_enriched_stocks(&state, &symbol, &components).await {
        Ok(stocks) if !stocks.is_empty() => stocks,
        Ok(_) | Err(_) => fallback_stocks,
    };

    let template = StockRowsFragment { stocks };
    Html(template.render().unwrap())
}

async fn get_enriched_stocks(
    state: &AppState,
    index_symbol: &str,
    components: &[IndexComponent],
) -> anyhow::Result<Vec<StockResult>> {
    if components.is_empty() {
        return Ok(Vec::new());
    }

    let cache_key = components
        .iter()
        .map(|component| component.symbol.as_str())
        .collect::<Vec<_>>()
        .join(",");

    {
        let cache = state.quote_cache.lock().await;
        if let Some(entry) = cache.get(&cache_key) {
            if entry.fetched_at.elapsed() < Duration::from_secs(900) {
                return Ok(entry.stocks.clone());
            }
        }
    }

    let symbols = components
        .iter()
        .map(|component| component.symbol.clone())
        .collect::<Vec<_>>();
    let fallback_names = components
        .iter()
        .map(|component| (component.symbol.clone(), component.name.clone()))
        .collect::<HashMap<_, _>>();
    let fallback_sectors = components
        .iter()
        .map(|component| (component.symbol.clone(), component.sector.clone()))
        .collect::<HashMap<_, _>>();

    let quotes_result = match state.scraper.get_quotes(&symbols).await {
        Ok(quotes) if !quotes.is_empty() => Ok(quotes),
        Ok(_) => get_quotes_from_python(symbols.clone()).await,
        Err(error) => {
            tracing::warn!(
                "Yahoo quote fetch failed for {}; trying Python bridge: {}",
                index_symbol,
                error
            );
            get_quotes_from_python(symbols.clone()).await
        }
    };

    let quotes = match quotes_result {
        Ok(quotes) => quotes,
        Err(error) => {
            tracing::warn!("Quote enrichment failed for {}: {}", index_symbol, error);
            return Ok(components
                .iter()
                .take(10)
                .cloned()
                .map(stock_from_component)
                .collect());
        }
    };

    let mut ranked_quotes = quotes
        .into_iter()
        .filter(|quote| quote.market_cap.is_some())
        .collect::<Vec<_>>();
    ranked_quotes.sort_by(|left, right| {
        right
            .market_cap
            .partial_cmp(&left.market_cap)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let stocks = ranked_quotes
        .iter()
        .take(10)
        .map(|quote| stock_from_quote(quote, &fallback_names, &fallback_sectors))
        .collect::<Vec<_>>();

    if stocks.is_empty() {
        return Ok(components
            .iter()
            .take(10)
            .cloned()
            .map(stock_from_component)
            .collect());
    }

    let mut cache = state.quote_cache.lock().await;
    cache.insert(
        cache_key,
        CachedStockRows {
            stocks: stocks.clone(),
            fetched_at: Instant::now(),
        },
    );

    Ok(stocks)
}

async fn get_quotes_from_python(symbols: Vec<String>) -> anyhow::Result<Vec<QuoteResult>> {
    let task = tokio::task::spawn_blocking(move || run_yfinance_bridge(&symbols));
    match tokio::time::timeout(Duration::from_secs(60), task).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(anyhow::anyhow!("Python quote task failed: {}", error)),
        Err(_) => Err(anyhow::anyhow!("Python quote bridge timed out")),
    }
}

async fn run_stock_research_bridge(
    symbol: &str,
    section: &str,
    model: &str,
) -> anyhow::Result<Value> {
    let symbol = symbol.to_string();
    let section = section.to_string();
    let model = model.to_string();
    let task = tokio::task::spawn_blocking(move || {
        run_stock_research_bridge_blocking(&symbol, &section, &model)
    });

    match tokio::time::timeout(Duration::from_secs(120), task).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(anyhow::anyhow!("Python research task failed: {}", error)),
        Err(_) => Err(anyhow::anyhow!("Python research bridge timed out")),
    }
}

fn run_stock_research_bridge_blocking(
    symbol: &str,
    section: &str,
    model: &str,
) -> anyhow::Result<Value> {
    let script = r#"
import json
import sys

symbol = sys.argv[1]
section = sys.argv[2]
model = sys.argv[3]

if section == "fundamentals":
    from stock_portfolio_ai.agents.fundamental_analyst_agent import FundamentalAnalystAgent
    report = FundamentalAnalystAgent(model=model).analyze_symbol_report(symbol).model_dump()
    print(json.dumps({"kind": "report", "title": "Fundamentals", "report": report}))
elif section == "technical":
    from stock_portfolio_ai.agents.technical_analyst_agent import TechnicalAnalystAgent
    report = TechnicalAnalystAgent(model=model).analyze_symbol_report(symbol).model_dump()
    print(json.dumps({"kind": "report", "title": "Technical Indicators", "report": report}))
elif section == "sentiment":
    from stock_portfolio_ai.agents.sentiment_analyst_agent import SentimentAnalystAgent
    report = SentimentAnalystAgent(model=model).analyze_symbol_report(symbol).model_dump()
    print(json.dumps({"kind": "report", "title": "Sentiment Analysis", "report": report}))
elif section == "market":
    from stock_portfolio_ai.agents.market_data_agent import MarketDataAgent
    agent = MarketDataAgent(model=model)
    price = agent.invoke_tool("get_stock_price", symbol=symbol)
    news = agent.invoke_tool("get_company_news", symbol=symbol)
    print(json.dumps({"kind": "market", "symbol": symbol, "price": price, "news": news}))
elif section == "conclusion":
    from stock_portfolio_ai.agents.supervisor_agent import SupervisorAgent
    summary = SupervisorAgent(model=model).analyze_symbol(symbol).model_dump()
    print(json.dumps({"kind": "conclusion", "summary": summary}))
else:
    raise ValueError(f"Unknown research section: {section}")
"#;

    let project_root = StdPath::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Could not resolve project root"))?;
    let uv_cache_dir = project_root.join(".uv-cache");

    let output = Command::new("uv")
        .current_dir(project_root)
        .env("UV_CACHE_DIR", path_to_string(&uv_cache_dir)?)
        .env("OPENROUTER_MODEL", model)
        .args(["run", "python", "-c", script, symbol, section, model])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Python research bridge failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}

fn render_research_payload(section: &str, payload: Value) -> Html<String> {
    match payload.get("kind").and_then(Value::as_str) {
        Some("report") => render_report_payload(payload),
        Some("market") => render_market_payload(payload),
        Some("conclusion") => render_conclusion_payload(payload),
        _ => Html(
            ResearchErrorFragment {
                message: format!("Unexpected research response for section '{section}'."),
            }
            .render()
            .unwrap(),
        ),
    }
}

fn render_report_payload(payload: Value) -> Html<String> {
    let report = payload.get("report").unwrap_or(&Value::Null);
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Research Report")
        .to_string();

    let evidence = report
        .get("evidence")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| EvidenceView {
                    label: string_value(item, "label"),
                    value: value_to_display(item.get("value")),
                    source: string_value(item, "source"),
                    explanation: string_value(item, "explanation"),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let template = ResearchReportFragment {
        title,
        rating: string_value(report, "rating").replace('_', " "),
        confidence: percent_value(report.get("confidence")),
        summary: string_value(report, "summary"),
        key_points: string_list(report.get("key_points")),
        risks: string_list(report.get("risks")),
        evidence,
    };

    Html(template.render().unwrap())
}

fn render_market_payload(payload: Value) -> Html<String> {
    let price = payload.get("price").unwrap_or(&Value::Null);
    let news = payload.get("news").unwrap_or(&Value::Null);

    let mut metrics = Vec::new();
    if price.get("error").is_some() {
        metrics.push(MetricView {
            label: "Price status".to_string(),
            value: string_value(price, "error"),
        });
    } else {
        metrics.push(MetricView {
            label: "Latest price".to_string(),
            value: value_to_display(price.get("price")),
        });
        metrics.push(MetricView {
            label: "Currency".to_string(),
            value: string_value(price, "currency"),
        });
        metrics.push(MetricView {
            label: "Exchange".to_string(),
            value: string_value(price, "exchange"),
        });
        metrics.push(MetricView {
            label: "Volume".to_string(),
            value: value_to_display(price.get("volume")),
        });
        metrics.push(MetricView {
            label: "As of".to_string(),
            value: string_value(price, "as_of"),
        });
    }

    let headlines = news
        .get("headlines")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| HeadlineView {
                    title: string_value(item, "title"),
                    publisher: string_value(item, "publisher"),
                    published_at: string_value(item, "published_at"),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Html(
        ResearchMarketFragment {
            symbol: string_value(&payload, "symbol"),
            metrics,
            headlines,
        }
        .render()
        .unwrap(),
    )
}

fn render_conclusion_payload(payload: Value) -> Html<String> {
    let summary = payload.get("summary").unwrap_or(&Value::Null);
    Html(
        ResearchConclusionFragment {
            conclusion: string_value(summary, "conclusion").replace('_', " "),
            confidence: percent_value(summary.get("confidence")),
            summary: string_value(summary, "summary"),
            key_findings: string_list(summary.get("key_findings")),
            risks: string_list(summary.get("risks")),
            conflicts: string_list(summary.get("conflicts")),
            next_steps: string_list(summary.get("next_steps")),
        }
        .render()
        .unwrap(),
    )
}

fn load_model_config() -> ModelConfig {
    let project_root = StdPath::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| StdPath::new(env!("CARGO_MANIFEST_DIR")));
    let config_path = project_root.join("config").join("openrouter_models.json");
    let fallback = ModelConfig {
        default_model: "openrouter/auto".to_string(),
        models: vec![ModelOption {
            id: "openrouter/auto".to_string(),
            label: "OpenRouter Auto".to_string(),
        }],
    };

    let Ok(content) = std::fs::read_to_string(config_path) else {
        return fallback;
    };

    serde_json::from_str(&content).unwrap_or(fallback)
}

fn model_option_views(config: &ModelConfig) -> Vec<ModelOptionView> {
    config
        .models
        .iter()
        .map(|model| ModelOptionView {
            id: model.id.clone(),
            label: model.label.clone(),
            selected: model.id == config.default_model,
        })
        .collect()
}

fn normalize_stock_symbol(symbol: &str) -> String {
    symbol.trim().to_uppercase()
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn string_value(value: &Value, key: &str) -> String {
    value
        .get(key)
        .map(Some)
        .map(value_to_display)
        .unwrap_or_else(|| "N/A".to_string())
}

fn value_to_display(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) if !value.is_empty() => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Null) | None => "N/A".to_string(),
        Some(other) => other.to_string(),
    }
}

fn percent_value(value: Option<&Value>) -> String {
    match value.and_then(Value::as_f64) {
        Some(value) => format!("{:.0}%", value * 100.0),
        None => "0%".to_string(),
    }
}

fn run_yfinance_bridge(symbols: &[String]) -> anyhow::Result<Vec<QuoteResult>> {
    let script = r#"
import json
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

import yfinance as yf

symbols = json.loads(sys.argv[1])

def fetch_quote(symbol):
    try:
        ticker = yf.Ticker(symbol)
        info = {}
        try:
            info = ticker.info or {}
        except Exception:
            info = {}

        fast = {}
        try:
            fast = dict(ticker.fast_info or {})
        except Exception:
            fast = {}

        return {
            "symbol": symbol,
            "name": info.get("longName") or info.get("shortName") or symbol,
            "market_cap": info.get("marketCap") or fast.get("market_cap"),
            "price": fast.get("last_price") or info.get("regularMarketPrice"),
            "currency": info.get("currency") or fast.get("currency"),
        }
    except Exception:
        return {
            "symbol": symbol,
            "name": symbol,
            "market_cap": None,
            "price": None,
            "currency": None,
        }

rows_by_symbol = {}
with ThreadPoolExecutor(max_workers=12) as executor:
    futures = {executor.submit(fetch_quote, symbol): symbol for symbol in symbols}
    for future in as_completed(futures):
        row = future.result()
        rows_by_symbol[row["symbol"]] = row

rows = [rows_by_symbol.get(symbol, {
    "symbol": symbol,
    "name": symbol,
    "market_cap": None,
    "price": None,
    "currency": None,
}) for symbol in symbols]

print(json.dumps(rows))
"#;

    let symbols_json = serde_json::to_string(symbols)?;
    let project_root = StdPath::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Could not resolve project root"))?;
    let uv_cache_dir = project_root.join(".uv-cache");

    let output = Command::new("uv")
        .current_dir(project_root)
        .env("UV_CACHE_DIR", path_to_string(&uv_cache_dir)?)
        .args(["run", "python", "-c", script, &symbols_json])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Python quote bridge failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let rows: Vec<PythonQuoteResult> = serde_json::from_slice(&output.stdout)?;
    Ok(rows.into_iter().map(QuoteResult::from).collect())
}

fn path_to_string(path: &PathBuf) -> anyhow::Result<String> {
    path.to_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8: {}", path.display()))
}

fn stock_from_quote(
    quote: &QuoteResult,
    fallback_names: &HashMap<String, String>,
    fallback_sectors: &HashMap<String, String>,
) -> StockResult {
    let name = if quote.name.trim().is_empty() || quote.name == quote.symbol {
        fallback_names
            .get(&quote.symbol)
            .cloned()
            .unwrap_or_else(|| quote.symbol.clone())
    } else {
        quote.name.clone()
    };

    StockResult {
        symbol: quote.symbol.clone(),
        name,
        sector: fallback_sectors
            .get(&quote.symbol)
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string()),
        market_cap: quote
            .market_cap
            .map(format_market_cap)
            .unwrap_or_else(|| "N/A".to_string()),
        price: quote
            .price
            .map(|price| match quote.currency.as_deref() {
                Some(currency) => format!("{price:.2} {currency}"),
                None => format!("{price:.2}"),
            })
            .unwrap_or_else(|| "N/A".to_string()),
    }
}

fn stock_from_component(component: IndexComponent) -> StockResult {
    StockResult {
        symbol: component.symbol,
        name: component.name,
        sector: component.sector,
        market_cap: "N/A".to_string(),
        price: "N/A".to_string(),
    }
}

fn format_market_cap(value: f64) -> String {
    if value >= 1_000_000_000_000.0 {
        format!("{:.2}T", value / 1_000_000_000_000.0)
    } else if value >= 1_000_000_000.0 {
        format!("{:.2}B", value / 1_000_000_000.0)
    } else if value >= 1_000_000.0 {
        format!("{:.2}M", value / 1_000_000.0)
    } else {
        format!("{value:.0}")
    }
}

fn mock_top_stocks() -> Vec<StockResult> {
    vec![
        StockResult {
            symbol: "AAPL".to_string(),
            name: "Apple Inc.".to_string(),
            sector: "Information Technology".to_string(),
            market_cap: "3.40T".to_string(),
            price: "225.10 USD".to_string(),
        },
        StockResult {
            symbol: "MSFT".to_string(),
            name: "Microsoft Corp.".to_string(),
            sector: "Information Technology".to_string(),
            market_cap: "3.10T".to_string(),
            price: "415.50 USD".to_string(),
        },
        StockResult {
            symbol: "NVDA".to_string(),
            name: "Nvidia Corp.".to_string(),
            sector: "Information Technology".to_string(),
            market_cap: "2.80T".to_string(),
            price: "120.30 USD".to_string(),
        },
        StockResult {
            symbol: "GOOGL".to_string(),
            name: "Alphabet Inc.".to_string(),
            sector: "Communication Services".to_string(),
            market_cap: "2.10T".to_string(),
            price: "175.20 USD".to_string(),
        },
        StockResult {
            symbol: "AMZN".to_string(),
            name: "Amazon.com Inc.".to_string(),
            sector: "Consumer Discretionary".to_string(),
            market_cap: "1.90T".to_string(),
            price: "180.10 USD".to_string(),
        },
        StockResult {
            symbol: "META".to_string(),
            name: "Meta Platforms".to_string(),
            sector: "Communication Services".to_string(),
            market_cap: "1.30T".to_string(),
            price: "490.20 USD".to_string(),
        },
        StockResult {
            symbol: "TSLA".to_string(),
            name: "Tesla Inc.".to_string(),
            sector: "Automobile".to_string(),
            market_cap: "700.00B".to_string(),
            price: "170.40 USD".to_string(),
        },
        StockResult {
            symbol: "BRK-B".to_string(),
            name: "Berkshire Hathaway".to_string(),
            sector: "Financial Services".to_string(),
            market_cap: "800.00B".to_string(),
            price: "410.10 USD".to_string(),
        },
        StockResult {
            symbol: "AVGO".to_string(),
            name: "Broadcom Inc.".to_string(),
            sector: "Information Technology".to_string(),
            market_cap: "600.00B".to_string(),
            price: "160.50 USD".to_string(),
        },
        StockResult {
            symbol: "LLY".to_string(),
            name: "Eli Lilly".to_string(),
            sector: "Healthcare".to_string(),
            market_cap: "550.00B".to_string(),
            price: "780.20 USD".to_string(),
        },
    ]
}

#[derive(Debug, Deserialize)]
struct PythonQuoteResult {
    symbol: String,
    name: String,
    market_cap: Option<f64>,
    price: Option<f64>,
    currency: Option<String>,
}

impl From<PythonQuoteResult> for QuoteResult {
    fn from(value: PythonQuoteResult) -> Self {
        Self {
            symbol: value.symbol,
            name: value.name,
            market_cap: value.market_cap,
            price: value.price,
            currency: value.currency,
        }
    }
}
