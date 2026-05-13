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
    market_cap: String,
    price: String,
}

// --- Handlers ---

async fn home_handler() -> impl IntoResponse {
    let template = IndexTemplate {};
    Html(template.render().unwrap())
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
        .route("/search/indices", post(search_indices_handler))
        .route("/index/:symbol/quotes", get(index_quotes_handler))
        .route("/index/:symbol", get(index_detail_handler))
        .route("/search/stocks/:symbol", post(search_stocks_handler))
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
        .map(|quote| stock_from_quote(quote, &fallback_names))
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

fn stock_from_quote(quote: &QuoteResult, fallback_names: &HashMap<String, String>) -> StockResult {
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
            market_cap: "3.40T".to_string(),
            price: "225.10 USD".to_string(),
        },
        StockResult {
            symbol: "MSFT".to_string(),
            name: "Microsoft Corp.".to_string(),
            market_cap: "3.10T".to_string(),
            price: "415.50 USD".to_string(),
        },
        StockResult {
            symbol: "NVDA".to_string(),
            name: "Nvidia Corp.".to_string(),
            market_cap: "2.80T".to_string(),
            price: "120.30 USD".to_string(),
        },
        StockResult {
            symbol: "GOOGL".to_string(),
            name: "Alphabet Inc.".to_string(),
            market_cap: "2.10T".to_string(),
            price: "175.20 USD".to_string(),
        },
        StockResult {
            symbol: "AMZN".to_string(),
            name: "Amazon.com Inc.".to_string(),
            market_cap: "1.90T".to_string(),
            price: "180.10 USD".to_string(),
        },
        StockResult {
            symbol: "META".to_string(),
            name: "Meta Platforms".to_string(),
            market_cap: "1.30T".to_string(),
            price: "490.20 USD".to_string(),
        },
        StockResult {
            symbol: "TSLA".to_string(),
            name: "Tesla Inc.".to_string(),
            market_cap: "700.00B".to_string(),
            price: "170.40 USD".to_string(),
        },
        StockResult {
            symbol: "BRK-B".to_string(),
            name: "Berkshire Hathaway".to_string(),
            market_cap: "800.00B".to_string(),
            price: "410.10 USD".to_string(),
        },
        StockResult {
            symbol: "AVGO".to_string(),
            name: "Broadcom Inc.".to_string(),
            market_cap: "600.00B".to_string(),
            price: "160.50 USD".to_string(),
        },
        StockResult {
            symbol: "LLY".to_string(),
            name: "Eli Lilly".to_string(),
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
