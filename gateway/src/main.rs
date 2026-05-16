use askama::Template;
use axum::{
    extract::{Form, Multipart, Path, State},
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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
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

#[derive(Clone)]
struct UploadedPortfolioFile {
    filename: String,
    bytes: Vec<u8>,
}

// --- Templates ---

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {}

#[derive(Template)]
#[template(path = "portfolio.html")]
struct PortfolioTemplate {
    group: PortfolioDashboardGroup,
}

#[derive(Template)]
#[template(path = "portfolio_tab_content_fragment.html")]
struct PortfolioTabContentFragment {
    group: PortfolioDashboardGroup,
}

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
    metrics: Vec<MetricView>,
    headlines: Vec<HeadlineView>,
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

#[derive(Clone, Deserialize)]
struct PortfolioDashboardGroup {
    mutual_funds: PortfolioDashboard,
    stocks: PortfolioDashboard,
    other_assets: PortfolioDashboard,
    stored_file_count: usize,
    uploaded_file_count: usize,
}

#[derive(Clone, Deserialize)]
struct PortfolioDashboard {
    has_data: bool,
    dashboard_type: String,
    title: String,
    primary_label: String,
    secondary_label: String,
    net_label: String,
    count_label: String,
    asset_title: String,
    amc_title: String,
    product_title: String,
    yearly_title: String,
    detail_title: String,
    detail_subtitle: String,
    next_data_note: String,
    net_invested: String,
    purchases: String,
    redemptions: String,
    transaction_count: usize,
    fund_count: usize,
    amc_count: usize,
    asset_rows: Vec<PortfolioBreakdownRow>,
    amc_rows: Vec<PortfolioBreakdownRow>,
    product_rows: Vec<PortfolioBreakdownRow>,
    fund_rows: Vec<PortfolioBreakdownRow>,
    yearly_rows: Vec<PortfolioBreakdownRow>,
}

#[derive(Clone, Deserialize)]
struct PortfolioBreakdownRow {
    label: String,
    amount: String,
    percent: String,
    transactions: usize,
}

fn empty_portfolio_group() -> PortfolioDashboardGroup {
    PortfolioDashboardGroup {
        mutual_funds: empty_portfolio_dashboard(
            "Mutual Funds",
            "Mutual Fund Transaction Dashboard",
            "Upload mutual fund order-history files to populate this tab.",
        ),
        stocks: empty_portfolio_dashboard(
            "Stocks",
            "Direct Stock Holdings Dashboard",
            "Upload stock holdings statements to populate this tab.",
        ),
        other_assets: empty_portfolio_dashboard(
            "Other Assets",
            "Other Asset Dashboard",
            "Debt, bonds, gold, silver, and other asset statements will populate here once source-specific parsers are added.",
        ),
        stored_file_count: 0,
        uploaded_file_count: 0,
    }
}

fn empty_portfolio_dashboard(
    dashboard_type: &str,
    title: &str,
    next_data_note: &str,
) -> PortfolioDashboard {
    PortfolioDashboard {
        has_data: false,
        dashboard_type: dashboard_type.to_string(),
        title: title.to_string(),
        primary_label: "Primary".to_string(),
        secondary_label: "Secondary".to_string(),
        net_label: "Total".to_string(),
        count_label: "Items".to_string(),
        asset_title: "Asset Split".to_string(),
        amc_title: "Allocation".to_string(),
        product_title: "Product Split".to_string(),
        yearly_title: "Timeline".to_string(),
        detail_title: "Detail View".to_string(),
        detail_subtitle: "No data uploaded for this tab".to_string(),
        next_data_note: next_data_note.to_string(),
        net_invested: "₹0".to_string(),
        purchases: "₹0".to_string(),
        redemptions: "₹0".to_string(),
        transaction_count: 0,
        fund_count: 0,
        amc_count: 0,
        asset_rows: Vec::new(),
        amc_rows: Vec::new(),
        product_rows: Vec::new(),
        fund_rows: Vec::new(),
        yearly_rows: Vec::new(),
    }
}

// --- Handlers ---

async fn home_handler() -> impl IntoResponse {
    let template = IndexTemplate {};
    Html(template.render().unwrap())
}

async fn portfolio_handler() -> impl IntoResponse {
    Html(
        PortfolioTemplate {
            group: empty_portfolio_group(),
        }
        .render()
        .unwrap(),
    )
}

async fn portfolio_upload_handler(mut multipart: Multipart) -> impl IntoResponse {
    let mut uploaded_files = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("portfolio_file") {
            if uploaded_files.len() >= 3 {
                return Html(
                    ResearchErrorFragment {
                        message: "Upload up to 3 files at a time.".to_string(),
                    }
                    .render()
                    .unwrap(),
                )
                .into_response();
            }

            let filename = field
                .file_name()
                .map(String::from)
                .unwrap_or_else(|| "portfolio.xlsx".to_string());
            match field.bytes().await {
                Ok(bytes) => uploaded_files.push(UploadedPortfolioFile {
                    filename,
                    bytes: bytes.to_vec(),
                }),
                Err(error) => {
                    return Html(
                        ResearchErrorFragment {
                            message: format!("Could not read uploaded workbook: {error}"),
                        }
                        .render()
                        .unwrap(),
                    )
                    .into_response();
                }
            }
        }
    }

    if uploaded_files.is_empty() {
        return Html(
            ResearchErrorFragment {
                message: "Choose at least one Excel file to upload.".to_string(),
            }
            .render()
            .unwrap(),
        )
        .into_response();
    }

    match analyze_portfolio_workbooks(&uploaded_files).await {
        Ok(group) => Html(PortfolioTabContentFragment { group }.render().unwrap()).into_response(),
        Err(error) => Html(
            ResearchErrorFragment {
                message: format!("Portfolio upload failed: {error}"),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
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
        .route("/portfolio", get(portfolio_handler))
        .route("/portfolio/upload", post(portfolio_upload_handler))
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

async fn analyze_portfolio_workbooks(
    files: &[UploadedPortfolioFile],
) -> anyhow::Result<PortfolioDashboardGroup> {
    let mut upload_paths = Vec::new();
    let mut manifest = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let extension = file
            .filename
            .rsplit_once('.')
            .map(|(_, extension)| extension)
            .unwrap_or("xlsx");
        let upload_path = temp_upload_path(&format!("portfolio-upload-{index}"), extension)?;
        std::fs::write(&upload_path, &file.bytes)?;
        manifest.push(serde_json::json!({
            "filename": file.filename,
            "path": path_to_string(&upload_path)?,
        }));
        upload_paths.push(upload_path);
    }

    let manifest_json = serde_json::to_string(&manifest)?;
    let uploaded_file_count = files.len();
    let project_root = StdPath::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Could not resolve project root"))?;
    let db_path = project_root.join("data").join("portfolio.sqlite");
    let task = tokio::task::spawn_blocking(move || {
        analyze_portfolio_workbooks_blocking(&manifest_json, &db_path, uploaded_file_count)
    });

    let result = match tokio::time::timeout(Duration::from_secs(90), task).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(anyhow::anyhow!("Portfolio parser task failed: {}", error)),
        Err(_) => Err(anyhow::anyhow!("Portfolio parser timed out")),
    };

    for path in upload_paths {
        let _ = std::fs::remove_file(path);
    }

    result
}

fn analyze_portfolio_workbooks_blocking(
    manifest_json: &str,
    db_path: &PathBuf,
    uploaded_file_count: usize,
) -> anyhow::Result<PortfolioDashboardGroup> {
    let script = r#"
import json
import hashlib
import pathlib
import re
import sqlite3
import sys

import pandas as pd

uploaded_file_count = int(sys.argv[1])
db_path = pathlib.Path(sys.argv[2])
manifest = json.loads(sys.argv[3])
db_path.parent.mkdir(parents=True, exist_ok=True)
conn = sqlite3.connect(db_path)
conn.row_factory = sqlite3.Row

conn.executescript("""
CREATE TABLE IF NOT EXISTS uploaded_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_hash TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    uploaded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS investments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    upload_id INTEGER NOT NULL REFERENCES uploaded_files(id),
    asset_class TEXT NOT NULL,
    source_file TEXT NOT NULL,
    broker TEXT,
    instrument_name TEXT NOT NULL,
    isin TEXT,
    transaction_type TEXT,
    quantity REAL,
    nav REAL,
    invested_value REAL,
    current_value REAL,
    pnl REAL,
    transaction_date TEXT,
    amc TEXT,
    product TEXT,
    asset_bucket TEXT,
    sector TEXT,
    market_cap_bucket TEXT,
    raw_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_investments_asset_class ON investments(asset_class);
CREATE INDEX IF NOT EXISTS idx_investments_isin ON investments(isin);
CREATE INDEX IF NOT EXISTS idx_investments_instrument ON investments(instrument_name);
""")

def money_to_float(value):
    if pd.isna(value):
        return 0.0
    if isinstance(value, (int, float)):
        return float(value)
    cleaned = re.sub(r"[^0-9.-]", "", str(value))
    return float(cleaned) if cleaned else 0.0

def parse_date(value):
    parsed = pd.to_datetime(value, dayfirst=True, errors="coerce")
    return parsed

def inr(value):
    return "₹{:,.0f}".format(float(value))

def find_header(raw, required):
    for idx, row in raw.iterrows():
        values = [str(value).strip() for value in row.tolist()]
        if all(item in values for item in required):
            return idx
    return None

def rows_from_grouped(grouped, label_column, amount_column="amount", count_column="transactions"):
    denominator = abs(float(grouped[amount_column].sum())) or 1.0
    rows = []
    for _, row in grouped.iterrows():
        amount = float(row[amount_column])
        rows.append({
            "label": str(row[label_column]),
            "amount": inr(amount),
            "percent": "{:.1f}%".format((abs(amount) / denominator) * 100),
            "transactions": int(row[count_column]),
        })
    return rows

def amc_from_scheme(name):
    name = str(name).strip()
    known = [
        "ICICI Prudential",
        "HDFC",
        "SBI",
        "Axis",
        "Bandhan",
        "Nippon India",
        "Kotak",
        "Aditya Birla Sun Life",
        "Mirae Asset",
        "UTI",
        "Motilal Oswal",
        "Parag Parikh",
        "DSP",
    ]
    lowered = name.lower()
    for amc in known:
        if lowered.startswith(amc.lower()):
            return amc
    return name.split()[0] if name else "Unknown"

def product_from_scheme(name):
    lowered = str(name).lower()
    if "nifty 50" in lowered:
        return "Nifty 50 Index"
    if "sensex" in lowered:
        return "Sensex Index"
    if "index" in lowered:
        return "Index Fund"
    if "large & mid cap" in lowered or "large and mid cap" in lowered:
        return "Large & Mid Cap"
    if "gold" in lowered:
        return "Gold"
    if "etf" in lowered:
        return "ETF/FoF"
    return "Other Mutual Fund"

def asset_from_scheme(name):
    lowered = str(name).lower()
    if "gold" in lowered:
        return "Gold"
    if "nifty" in lowered or "sensex" in lowered or "index" in lowered:
        return "Equity Index"
    if "large" in lowered or "mid cap" in lowered or "equity" in lowered:
        return "Equity Active"
    if "debt" in lowered or "liquid" in lowered or "bond" in lowered:
        return "Debt"
    return "Other"

def mutual_breakdown(df, column, limit=None):
    grouped = (
        df.groupby(column, dropna=False)
        .agg(amount=("SignedAmount", "sum"), transactions=("Scheme Name", "count"))
        .reset_index()
    )
    grouped["sort_amount"] = grouped["amount"].abs()
    grouped = grouped.sort_values("sort_amount", ascending=False)
    if limit:
        grouped = grouped.head(limit)
    return rows_from_grouped(grouped, column)

def sector_from_stock(name):
    lowered = str(name).lower()
    if "bank" in lowered or "kotak" in lowered or "hdfc bank" in lowered or "idfc" in lowered:
        return "Financials"
    if "finance" in lowered or "cards" in lowered or "life" in lowered:
        return "Financial Services"
    if "paint" in lowered:
        return "Paints"
    if "dabur" in lowered or "hindustan unilever" in lowered or "itc" in lowered or "avenue" in lowered:
        return "Consumer"
    if "tata motors" in lowered:
        return "Automobile"
    if "pharma" in lowered:
        return "Pharmaceuticals"
    if "blue star" in lowered or "voltas" in lowered or "v-guard" in lowered or "symphony" in lowered:
        return "Consumer Durables"
    if "polymer" in lowered or "ferrous" in lowered or "agro" in lowered:
        return "Materials"
    if "gabriel" in lowered:
        return "Industrials"
    return "Unclassified"

def market_cap_bucket(name):
    lowered = str(name).lower()
    large = ["asian paints", "avenue", "bajaj finance", "dabur", "hdfc bank", "hdfc life", "hindustan unilever", "itc", "kotak", "sbi cards", "tata motors", "voltas"]
    if any(item in lowered for item in large):
        return "Large Cap"
    mid = ["blue star", "idfc", "v-guard", "symphony", "gabriel"]
    if any(item in lowered for item in mid):
        return "Mid Cap"
    return "Small/Other Cap"

def stock_breakdown(df, column, limit=None):
    grouped = (
        df.groupby(column, dropna=False)
        .agg(amount=("ClosingValue", "sum"), transactions=("Stock Name", "count"))
        .reset_index()
        .sort_values("amount", ascending=False)
    )
    if limit:
        grouped = grouped.head(limit)
    return rows_from_grouped(grouped, column)

def empty_dashboard(kind, title, note):
    return {
        "has_data": False,
        "dashboard_type": kind,
        "title": title,
        "primary_label": "Primary",
        "secondary_label": "Secondary",
        "net_label": "Total",
        "count_label": "Items",
        "asset_title": "Asset Split",
        "amc_title": "Allocation",
        "product_title": "Product Split",
        "yearly_title": "Timeline",
        "detail_title": "Detail View",
        "detail_subtitle": "No data uploaded for this tab",
        "next_data_note": note,
        "net_invested": "₹0",
        "purchases": "₹0",
        "redemptions": "₹0",
        "transaction_count": 0,
        "fund_count": 0,
        "amc_count": 0,
        "asset_rows": [],
        "amc_rows": [],
        "product_rows": [],
        "fund_rows": [],
        "yearly_rows": [],
    }

def file_hash(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

def parse_file(path):
    raw = pd.read_excel(path, sheet_name=0, header=None)
    mutual_header = find_header(raw, ["Scheme Name", "Transaction Type"])
    stock_header = find_header(raw, ["Stock Name", "ISIN", "Quantity"])
    if mutual_header is not None:
        headers = [str(value).strip() for value in raw.iloc[mutual_header].tolist()]
        df = raw.iloc[mutual_header + 1:].copy()
        df.columns = headers
        df = df.dropna(how="all")
        df = df[df["Scheme Name"].notna()]
        df = df[df["Transaction Type"].notna()]
        df["SourceFile"] = path
        return "mutual_fund", df
    if stock_header is not None:
        headers = [str(value).strip() for value in raw.iloc[stock_header].tolist()]
        df = raw.iloc[stock_header + 1:].copy()
        df.columns = headers
        df = df.dropna(how="all")
        df = df[df["ISIN"].notna()]
        df["SourceFile"] = path
        return "stock", df
    return "other_assets", pd.DataFrame([{"SourceFile": path, "Amount": 0.0}])

def insert_file(item):
    path = item["path"]
    filename = item["filename"]
    digest = file_hash(path)
    existing = conn.execute("SELECT id FROM uploaded_files WHERE file_hash = ?", (digest,)).fetchone()
    if existing:
        return

    kind, df = parse_file(path)
    cursor = conn.execute(
        "INSERT INTO uploaded_files (file_hash, filename, asset_class) VALUES (?, ?, ?)",
        (digest, filename, kind),
    )
    upload_id = cursor.lastrowid

    if kind == "mutual_fund":
        df["AmountValue"] = df["Amount"].map(money_to_float)
        df["SignedAmount"] = df.apply(
            lambda row: -row["AmountValue"] if str(row["Transaction Type"]).strip().upper() == "REDEEM" else row["AmountValue"],
            axis=1,
        )
        df["AMC"] = df["Scheme Name"].map(amc_from_scheme)
        df["Product"] = df["Scheme Name"].map(product_from_scheme)
        df["Asset"] = df["Scheme Name"].map(asset_from_scheme)
        df["ParsedDate"] = df["Date"].map(parse_date)
        for _, row in df.iterrows():
            parsed_date = row["ParsedDate"]
            transaction_date = None if pd.isna(parsed_date) else parsed_date.date().isoformat()
            conn.execute(
                """
                INSERT INTO investments (
                    upload_id, asset_class, source_file, broker, instrument_name, transaction_type,
                    quantity, nav, invested_value, transaction_date, amc, product, asset_bucket, raw_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    upload_id,
                    "mutual_fund",
                    filename,
                    "Groww",
                    str(row["Scheme Name"]),
                    str(row["Transaction Type"]).strip().upper(),
                    money_to_float(row.get("Units")),
                    money_to_float(row.get("NAV")),
                    float(row["SignedAmount"]),
                    transaction_date,
                    str(row["AMC"]),
                    str(row["Product"]),
                    str(row["Asset"]),
                    row.to_json(default_handler=str),
                ),
            )
    elif kind == "stock":
        df["Stock Name"] = df["Stock Name"].fillna("Unknown Stock")
        df["BuyValue"] = df["Buy value"].map(money_to_float)
        df["ClosingValue"] = df["Closing value"].map(money_to_float)
        df["UnrealisedValue"] = df["Unrealised P&L"].map(money_to_float)
        df = df[df["ClosingValue"].notna()]
        df["Sector"] = df["Stock Name"].map(sector_from_stock)
        df["MarketCap"] = df["Stock Name"].map(market_cap_bucket)
        for _, row in df.iterrows():
            conn.execute(
                """
                INSERT INTO investments (
                    upload_id, asset_class, source_file, broker, instrument_name, isin,
                    quantity, invested_value, current_value, pnl, asset_bucket, sector,
                    market_cap_bucket, raw_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    upload_id,
                    "stock",
                    filename,
                    "Groww",
                    str(row["Stock Name"]),
                    str(row["ISIN"]),
                    money_to_float(row.get("Quantity")),
                    float(row["BuyValue"]),
                    float(row["ClosingValue"]),
                    float(row["UnrealisedValue"]),
                    "Direct Equity",
                    str(row["Sector"]),
                    str(row["MarketCap"]),
                    row.to_json(default_handler=str),
                ),
            )
    conn.commit()

for item in manifest:
    insert_file(item)

def build_mutual_dashboard():
    df = pd.read_sql_query(
        "SELECT * FROM investments WHERE asset_class = 'mutual_fund'",
        conn,
    )
    if df.empty:
        return empty_dashboard(
            "Mutual Funds",
            "Mutual Fund Transaction Dashboard",
            "Upload mutual fund order-history files to populate this tab. Market cap split and sector allocation require scheme holdings from factsheets or a holdings provider.",
        )
    df["year"] = pd.to_datetime(df["transaction_date"], errors="coerce").dt.year.fillna(0).astype(int).astype(str).replace({"0": "Unknown"})
    purchase_mask = df["transaction_type"].astype(str).str.upper().eq("PURCHASE")
    redeem_mask = df["transaction_type"].astype(str).str.upper().eq("REDEEM")
    purchases = float(df.loc[purchase_mask, "invested_value"].abs().sum())
    redemptions = float(df.loc[redeem_mask, "invested_value"].abs().sum())
    net = purchases - redemptions
    return {
        "has_data": True,
        "dashboard_type": "Mutual Funds",
        "title": "Mutual Fund Transaction Dashboard",
        "primary_label": "Total purchases",
        "secondary_label": "Redemptions",
        "net_label": "Net invested",
        "count_label": "Funds / AMCs",
        "asset_title": "Asset Split",
        "amc_title": "AMC Split",
        "product_title": "Product Split",
        "yearly_title": "Yearly Flow",
        "detail_title": "Fund-Level View",
        "detail_subtitle": "Top funds by net transaction amount",
        "next_data_note": "Market cap split and sector allocation need fund holdings or a scheme-to-holdings reference table. This order-history file can identify AMC, product, asset category, fund, dates, and transaction flows, but it does not contain the underlying stocks or sector weights.",
        "net_invested": inr(net),
        "purchases": inr(purchases),
        "redemptions": inr(redemptions),
        "transaction_count": int(len(df)),
        "fund_count": int(df["instrument_name"].nunique()),
        "amc_count": int(df["amc"].nunique()),
        "asset_rows": mutual_breakdown(df.rename(columns={"asset_bucket": "Asset", "instrument_name": "Scheme Name", "invested_value": "SignedAmount"}), "Asset"),
        "amc_rows": mutual_breakdown(df.rename(columns={"amc": "AMC", "instrument_name": "Scheme Name", "invested_value": "SignedAmount"}), "AMC"),
        "product_rows": mutual_breakdown(df.rename(columns={"product": "Product", "instrument_name": "Scheme Name", "invested_value": "SignedAmount"}), "Product"),
        "fund_rows": mutual_breakdown(df.rename(columns={"instrument_name": "Scheme Name", "invested_value": "SignedAmount"}), "Scheme Name", limit=10),
        "yearly_rows": mutual_breakdown(df.rename(columns={"year": "Year", "instrument_name": "Scheme Name", "invested_value": "SignedAmount"}), "Year"),
    }

def build_stock_dashboard():
    df = pd.read_sql_query(
        "SELECT * FROM investments WHERE asset_class = 'stock'",
        conn,
    )
    if df.empty:
        return empty_dashboard(
            "Stocks",
            "Direct Stock Holdings Dashboard",
            "Upload stock holdings statements to populate this tab. Production enrichment should resolve sector and market-cap buckets from ISIN/security master data.",
        )
    df["Stock Name"] = df["instrument_name"].fillna("Unknown Stock")
    df["BuyValue"] = df["invested_value"].fillna(0.0)
    df["ClosingValue"] = df["current_value"].fillna(0.0)
    df["UnrealisedValue"] = df["pnl"].fillna(0.0)
    df["Asset"] = df["asset_bucket"].fillna("Direct Equity")
    df["Sector"] = df["sector"].fillna("Unclassified")
    df["MarketCap"] = df["market_cap_bucket"].fillna("Unclassified")
    invested = float(df["BuyValue"].sum())
    closing = float(df["ClosingValue"].sum())
    pnl = float(df["UnrealisedValue"].sum())
    return {
        "has_data": True,
        "dashboard_type": "Stocks",
        "title": "Direct Stock Holdings Dashboard",
        "primary_label": "Invested value",
        "secondary_label": "Unrealised P&L",
        "net_label": "Closing value",
        "count_label": "Stocks / ISINs",
        "asset_title": "Asset Split",
        "amc_title": "Sector Allocation",
        "product_title": "Market Cap Split",
        "yearly_title": "P&L Buckets",
        "detail_title": "Stock-Level View",
        "detail_subtitle": "Top holdings by closing value",
        "next_data_note": "This stock holdings file has direct holdings and values. Sector and market-cap buckets are classified locally from stock names for now; production should enrich each ISIN from exchange/security masters or a market data provider.",
        "net_invested": inr(closing),
        "purchases": inr(invested),
        "redemptions": inr(pnl),
        "transaction_count": int(len(df)),
        "fund_count": int(df["Stock Name"].nunique()),
        "amc_count": int(df["isin"].nunique()),
        "asset_rows": stock_breakdown(df, "Asset"),
        "amc_rows": stock_breakdown(df, "Sector"),
        "product_rows": stock_breakdown(df, "MarketCap"),
        "fund_rows": stock_breakdown(df, "Stock Name", limit=12),
        "yearly_rows": rows_from_grouped(
            pd.DataFrame([
                {"Bucket": "Gains", "amount": float(df.loc[df["UnrealisedValue"] > 0, "UnrealisedValue"].sum()), "transactions": int((df["UnrealisedValue"] > 0).sum())},
                {"Bucket": "Losses", "amount": float(df.loc[df["UnrealisedValue"] < 0, "UnrealisedValue"].sum()), "transactions": int((df["UnrealisedValue"] < 0).sum())},
                {"Bucket": "Flat", "amount": float(df.loc[df["UnrealisedValue"] == 0, "ClosingValue"].sum()), "transactions": int((df["UnrealisedValue"] == 0).sum())},
            ]),
            "Bucket",
        ),
    }

def build_other_dashboard():
    other_files = pd.read_sql_query(
        "SELECT * FROM uploaded_files WHERE asset_class = 'other_assets'",
        conn,
    )
    if other_files.empty:
        return empty_dashboard(
            "Other Assets",
            "Other Asset Dashboard",
            "Debt, bonds, gold, silver, and other asset statements will populate here once source-specific parsers are added.",
        )
    grouped = pd.DataFrame([{
        "Asset": "Unclassified files",
        "amount": 0.0,
        "transactions": len(other_files),
    }])
    dashboard = empty_dashboard(
        "Other Assets",
        "Other Asset Dashboard",
        "These uploaded files could not be classified yet. Add parser support or route them through an LLM schema-mapping fallback.",
    )
    dashboard["has_data"] = True
    dashboard["transaction_count"] = len(other_files)
    dashboard["asset_rows"] = rows_from_grouped(grouped, "Asset")
    dashboard["detail_subtitle"] = "Unclassified uploaded files"
    return dashboard

stored_file_count = int(conn.execute("SELECT COUNT(*) FROM uploaded_files").fetchone()[0])
group = {
    "mutual_funds": build_mutual_dashboard(),
    "stocks": build_stock_dashboard(),
    "other_assets": build_other_dashboard(),
    "stored_file_count": stored_file_count,
    "uploaded_file_count": uploaded_file_count,
}

print(json.dumps(group))
"#;

    let project_root = StdPath::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Could not resolve project root"))?;
    let uv_cache_dir = project_root.join(".uv-cache");
    let uploaded_file_count_arg = uploaded_file_count.to_string();
    let db_path_arg = path_to_string(db_path)?;

    let output = Command::new("uv")
        .current_dir(project_root)
        .env("UV_CACHE_DIR", path_to_string(&uv_cache_dir)?)
        .args([
            "run",
            "--with",
            "openpyxl",
            "python",
            "-c",
            script,
            &uploaded_file_count_arg,
            &db_path_arg,
            manifest_json,
        ])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Python portfolio parser failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}

fn temp_upload_path(prefix: &str, extension: &str) -> anyhow::Result<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(std::env::temp_dir().join(format!("{prefix}-{nanos}.{extension}")))
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
    from stock_portfolio_ai.agents.market_data_agent import MarketDataAgent
    report = TechnicalAnalystAgent(model=model).analyze_symbol_report(symbol).model_dump()
    price = MarketDataAgent(model=model).invoke_tool("get_stock_price", symbol=symbol)
    print(json.dumps({"kind": "report", "title": "Technical Indicators", "report": report, "price": price}))
elif section == "sentiment":
    from stock_portfolio_ai.agents.sentiment_analyst_agent import SentimentAnalystAgent
    from stock_portfolio_ai.agents.market_data_agent import MarketDataAgent
    report = SentimentAnalystAgent(model=model).analyze_symbol_report(symbol).model_dump()
    news = MarketDataAgent(model=model).invoke_tool("get_company_news", symbol=symbol)
    print(json.dumps({"kind": "report", "title": "Sentiment Analysis", "report": report, "news": news}))
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

    let metrics = market_metrics(payload.get("price").unwrap_or(&Value::Null));
    let headlines = market_headlines(payload.get("news").unwrap_or(&Value::Null), 5);

    let template = ResearchReportFragment {
        title,
        rating: string_value(report, "rating").replace('_', " "),
        confidence: percent_value(report.get("confidence")),
        summary: string_value(report, "summary"),
        key_points: string_list(report.get("key_points")),
        risks: string_list(report.get("risks")),
        evidence,
        metrics,
        headlines,
    };

    Html(template.render().unwrap())
}

fn render_market_payload(payload: Value) -> Html<String> {
    let price = payload.get("price").unwrap_or(&Value::Null);
    let news = payload.get("news").unwrap_or(&Value::Null);

    Html(
        ResearchMarketFragment {
            symbol: string_value(&payload, "symbol"),
            metrics: market_metrics(price),
            headlines: market_headlines(news, usize::MAX),
        }
        .render()
        .unwrap(),
    )
}

fn market_metrics(price: &Value) -> Vec<MetricView> {
    let mut metrics = Vec::new();
    if price.is_null() {
        return metrics;
    }

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

    metrics
}

fn market_headlines(news: &Value, limit: usize) -> Vec<HeadlineView> {
    news.get("headlines")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(limit)
                .map(|item| HeadlineView {
                    title: string_value(item, "title"),
                    publisher: string_value(item, "publisher"),
                    published_at: string_value(item, "published_at"),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn render_conclusion_payload(payload: Value) -> Html<String> {
    let summary = payload.get("summary").unwrap_or(&Value::Null);
    Html(
        ResearchConclusionFragment {
            conclusion: conclusion_to_rating(&string_value(summary, "conclusion")),
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

fn conclusion_to_rating(conclusion: &str) -> String {
    match conclusion {
        "bullish" | "bearish" | "neutral" => conclusion.to_string(),
        "favorable" => "bullish".to_string(),
        "unfavorable" => "bearish".to_string(),
        "insufficient_data" => "insufficient data".to_string(),
        other => other.replace('_', " "),
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
