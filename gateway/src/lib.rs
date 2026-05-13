use anyhow::{anyhow, Result};
use itertools::Itertools;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct IndexComponent {
    pub symbol: String,
    pub name: String,
    pub sector: String,
}

#[derive(Clone, Debug)]
pub struct QuoteResult {
    pub symbol: String,
    pub name: String,
    pub market_cap: Option<f64>,
    pub price: Option<f64>,
    pub currency: Option<String>,
}

struct CachedResult {
    components: Vec<IndexComponent>,
    fetched_at: Instant,
}

pub struct IndexScraper {
    client: Client,
    cache: Arc<Mutex<HashMap<String, CachedResult>>>,
    sectors: SectorResolver,
}

impl IndexScraper {
    pub fn new() -> Self {
        let project_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")));
        let sector_path = project_root.join("data").join("sector_overrides.csv");

        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
                .build()
                .expect("Failed to create HTTP client"),
            cache: Arc::new(Mutex::new(HashMap::new())),
            sectors: SectorResolver::from_csv_file(&sector_path),
        }
    }

    pub async fn get_index_components(&self, symbol: &str) -> Result<Vec<IndexComponent>> {
        {
            let cache = self.cache.lock().await;
            if let Some(entry) = cache.get(symbol) {
                if entry.fetched_at.elapsed() < Duration::from_secs(3600) {
                    return Ok(entry.components.clone());
                }
            }
        }

        let components = match symbol {
            "^GSPC" => {
                self.scrape_table(
                    "https://en.wikipedia.org/wiki/List_of_S%26P_500_companies",
                    &["Symbol"],
                    &["Security"],
                    None,
                )
                .await?
            }
            "^NDX" => {
                self.scrape_table(
                    "https://en.wikipedia.org/wiki/Nasdaq-100",
                    &["Ticker"],
                    &["Company"],
                    None,
                )
                .await?
            }
            "^NSEI" => {
                self.scrape_table(
                    "https://en.wikipedia.org/wiki/NIFTY_50",
                    &["Symbol"],
                    &["Company name", "Company"],
                    Some(".NS"),
                )
                .await?
            }
            "^FTSE" => {
                self.scrape_table(
                    "https://en.wikipedia.org/wiki/FTSE_100_Index",
                    &["Ticker"],
                    &["Company"],
                    Some(".L"),
                )
                .await?
            }
            "^N225" => self.scrape_nikkei225().await?,
            "^GDAXI" => {
                self.scrape_table(
                    "https://en.wikipedia.org/wiki/DAX",
                    &["Ticker", "Symbol"],
                    &["Company"],
                    Some(".DE"),
                )
                .await?
            }
            _ => return Err(anyhow!("Unknown index: {}", symbol)),
        };

        let mut cache = self.cache.lock().await;
        cache.insert(
            symbol.to_string(),
            CachedResult {
                components: components.clone(),
                fetched_at: Instant::now(),
            },
        );

        Ok(components)
    }

    pub async fn get_quotes(&self, symbols: &[String]) -> Result<Vec<QuoteResult>> {
        if symbols.is_empty() {
            return Ok(Vec::new());
        }

        let mut quotes = Vec::new();
        for chunk in symbols.chunks(50) {
            let response: YahooQuoteResponse = self
                .client
                .get("https://query1.finance.yahoo.com/v7/finance/quote")
                .query(&[("symbols", chunk.join(","))])
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

            quotes.extend(
                response
                    .quote_response
                    .result
                    .into_iter()
                    .map(QuoteResult::from),
            );
        }

        Ok(quotes)
    }

    async fn scrape_table(
        &self,
        url: &str,
        symbol_headers: &[&str],
        name_headers: &[&str],
        suffix: Option<&str>,
    ) -> Result<Vec<IndexComponent>> {
        let html = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?
            .replace("&amp;", "&");

        let doc = Html::parse_document(&html);
        let table_sel = Selector::parse("table.wikitable").unwrap();
        let row_sel = Selector::parse("tr").unwrap();
        let header_sel = Selector::parse("th").unwrap();
        let cell_sel = Selector::parse("td").unwrap();

        for table in doc.select(&table_sel) {
            let Some(header_row) = table.select(&row_sel).next() else {
                continue;
            };
            let headers: Vec<String> = header_row.select(&header_sel).map(clean_text).collect();

            let Some(symbol_index) = find_header_index(&headers, symbol_headers) else {
                continue;
            };
            let name_index = find_header_index(&headers, name_headers).unwrap_or(symbol_index);

            let components: Vec<IndexComponent> = table
                .select(&row_sel)
                .skip(1)
                .filter_map(|row| {
                    let cells: Vec<String> = row.select(&cell_sel).map(clean_text).collect();
                    let symbol = cells.get(symbol_index)?;
                    let name = cells.get(name_index).unwrap_or(symbol);
                    let normalized_symbol = normalize_symbol(symbol, suffix)?;
                    let sector = self.sectors.resolve(&normalized_symbol);

                    Some(IndexComponent {
                        symbol: normalized_symbol,
                        name: clean_company_name(name),
                        sector,
                    })
                })
                .unique_by(|component| component.symbol.clone())
                .collect();

            if !components.is_empty() {
                return Ok(components);
            }
        }

        Err(anyhow!("No component table found at {}", url))
    }

    async fn scrape_nikkei225(&self) -> Result<Vec<IndexComponent>> {
        let html = self
            .client
            .get("https://en.wikipedia.org/wiki/Nikkei_225")
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        let doc = Html::parse_document(&html);
        let item_sel = Selector::parse("li").unwrap();
        let link_sel = Selector::parse("a").unwrap();

        let components = doc
            .select(&item_sel)
            .filter_map(|item| {
                let links = item.select(&link_sel).collect::<Vec<_>>();
                let code = links
                    .iter()
                    .find(|link| {
                        link.value()
                            .attr("href")
                            .is_some_and(|href| href.contains("www2.jpx.co.jp"))
                    })
                    .map(|link| clean_text(*link))?;
                let name = links
                    .first()
                    .map(|link| clean_company_name(&clean_text(*link)))?;
                let symbol = normalize_symbol(&code, Some(".T"))?;

                Some(IndexComponent {
                    symbol: symbol.clone(),
                    name,
                    sector: self.sectors.resolve(&symbol),
                })
            })
            .unique_by(|component| component.symbol.clone())
            .collect::<Vec<_>>();

        if components.is_empty() {
            return Err(anyhow!("No Nikkei 225 components found"));
        }

        Ok(components)
    }
}

#[derive(Clone, Debug)]
struct SectorResolver {
    sectors_by_symbol: Arc<HashMap<String, String>>,
}

impl SectorResolver {
    fn from_csv_file(path: &Path) -> Self {
        let content = fs::read_to_string(path).unwrap_or_default();
        Self::from_csv_content(&content)
    }

    fn from_csv_content(content: &str) -> Self {
        let sectors_by_symbol = content
            .lines()
            .skip(1)
            .filter_map(parse_sector_row)
            .collect::<HashMap<_, _>>();

        Self {
            sectors_by_symbol: Arc::new(sectors_by_symbol),
        }
    }

    fn resolve(&self, symbol: &str) -> String {
        self.sectors_by_symbol
            .get(&symbol.to_uppercase())
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string())
    }
}

fn parse_sector_row(row: &str) -> Option<(String, String)> {
    let trimmed = row.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let mut columns = trimmed.splitn(2, ',');
    let symbol = columns.next()?.trim().to_uppercase();
    let sector = columns.next()?.trim().to_string();

    if symbol.is_empty() || sector.is_empty() {
        return None;
    }

    Some((symbol, sector))
}

impl Default for IndexScraper {
    fn default() -> Self {
        Self::new()
    }
}

fn find_header_index(headers: &[String], candidates: &[&str]) -> Option<usize> {
    headers.iter().position(|header| {
        let normalized = normalize_header(header);
        candidates
            .iter()
            .any(|candidate| normalized == normalize_header(candidate))
    })
}

fn normalize_header(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

fn clean_text(element: scraper::ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .join(" ")
        .trim()
        .to_string()
}

fn clean_company_name(value: &str) -> String {
    value.split('[').next().unwrap_or(value).trim().to_string()
}

fn normalize_symbol(value: &str, suffix: Option<&str>) -> Option<String> {
    let mut symbol = value
        .split_whitespace()
        .next()?
        .trim()
        .trim_matches('*')
        .trim()
        .to_uppercase();

    if symbol.is_empty() {
        return None;
    }

    if suffix.is_none() {
        symbol = symbol.replace('.', "-");
    }

    if !symbol
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '.')
    {
        return None;
    }

    if let Some(suffix) = suffix {
        if !symbol.contains('.') && !symbol.ends_with(suffix) {
            symbol.push_str(suffix);
        }
    }

    Some(symbol)
}

#[derive(Debug, Deserialize)]
struct YahooQuoteResponse {
    #[serde(rename = "quoteResponse")]
    quote_response: YahooQuoteEnvelope,
}

#[derive(Debug, Deserialize)]
struct YahooQuoteEnvelope {
    result: Vec<YahooQuote>,
}

#[derive(Debug, Deserialize)]
struct YahooQuote {
    symbol: String,
    #[serde(rename = "shortName")]
    short_name: Option<String>,
    #[serde(rename = "longName")]
    long_name: Option<String>,
    #[serde(rename = "marketCap")]
    market_cap: Option<f64>,
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
    currency: Option<String>,
}

impl From<YahooQuote> for QuoteResult {
    fn from(value: YahooQuote) -> Self {
        let name = value
            .long_name
            .or(value.short_name)
            .unwrap_or_else(|| value.symbol.clone());

        Self {
            symbol: value.symbol,
            name,
            market_cap: value.market_cap,
            price: value.regular_market_price,
            currency: value.currency,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SectorResolver;

    #[test]
    fn sector_resolver_uses_csv_override_and_unknown_fallback() {
        let resolver = SectorResolver::from_csv_content(
            "symbol,sector\nRELIANCE.NS,Energy\nTCS.NS,Information Technology\n",
        );

        assert_eq!(resolver.resolve("reliance.ns"), "Energy");
        assert_eq!(resolver.resolve("TCS.NS"), "Information Technology");
        assert_eq!(resolver.resolve("MISSING.NS"), "Unknown");
    }
}
