//! HTTP query parameter parsing and pagination for database queries.
//!
//! Mirrors the gokit `database/query/` package, providing:
//!
//! - [`QueryConfig`] — allowed sorts, filters, and page-size limits.
//! - [`QueryParams`] — parsed page/sort/filter values ready for SQL use.
//! - [`Pagination`] — response metadata (page, total, total_pages).
//! - [`PaginatedResult`] — generic wrapper pairing data with pagination.
//! - [`parse_query_string`] — parses a URL query string into [`QueryParams`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::FindOpts;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for query parameter parsing.
#[derive(Debug, Clone)]
pub struct QueryConfig {
    /// Default page size when not specified (default: 20).
    pub default_page_size: i64,
    /// Maximum allowed page size (default: 100).
    pub max_page_size: i64,
    /// Allowed sort columns. Empty means all are allowed.
    pub allowed_sorts: Vec<String>,
    /// Allowed filter columns. Empty means all are allowed.
    pub allowed_filters: Vec<String>,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            default_page_size: 20,
            max_page_size: 100,
            allowed_sorts: Vec::new(),
            allowed_filters: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Sort order
// ---------------------------------------------------------------------------

/// Sort direction for query results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    /// Ascending (default).
    #[default]
    Asc,
    /// Descending.
    Desc,
}

impl SortOrder {
    /// SQL keyword representation (`"ASC"` or `"DESC"`).
    pub fn as_sql(&self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }
}

impl std::fmt::Display for SortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_sql())
    }
}

// ---------------------------------------------------------------------------
// Parsed query parameters
// ---------------------------------------------------------------------------

/// Parsed query parameters from an HTTP request.
#[derive(Debug, Clone)]
pub struct QueryParams {
    /// Current page number (1-based).
    pub page: i64,
    /// Number of items per page.
    pub page_size: i64,
    /// Column to sort by (if any).
    pub sort_by: Option<String>,
    /// Sort direction.
    pub sort_order: SortOrder,
    /// Column-value filter pairs.
    pub filters: HashMap<String, String>,
}

impl QueryParams {
    /// SQL `LIMIT` value.
    pub fn limit(&self) -> i64 {
        self.page_size
    }

    /// SQL `OFFSET` value.
    pub fn offset(&self) -> i64 {
        (self.page - 1) * self.page_size
    }

    /// Convert to [`FindOpts`] for use with the repository layer.
    pub fn to_find_opts(&self) -> FindOpts {
        let mut opts = FindOpts::default()
            .with_limit(self.limit())
            .with_offset(self.offset());

        if let Some(ref col) = self.sort_by {
            opts = opts.order_by(&format!("{col} {}", self.sort_order));
        }

        for (col, val) in &self.filters {
            opts = opts.filter(col, val.clone());
        }

        opts
    }
}

// ---------------------------------------------------------------------------
// Pagination metadata
// ---------------------------------------------------------------------------

/// Pagination metadata for responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pagination {
    /// Current page number (1-based).
    pub page: i64,
    /// Items per page.
    pub page_size: i64,
    /// Total number of items across all pages.
    pub total: i64,
    /// Total number of pages.
    pub total_pages: i64,
}

impl Pagination {
    /// Create pagination metadata from page info and a total item count.
    pub fn new(page: i64, page_size: i64, total: i64) -> Self {
        let total_pages = if total == 0 || page_size == 0 {
            0
        } else {
            (total + page_size - 1) / page_size
        };
        Self {
            page,
            page_size,
            total,
            total_pages,
        }
    }
}

// ---------------------------------------------------------------------------
// Paginated result
// ---------------------------------------------------------------------------

/// Paginated result containing data and pagination metadata.
#[derive(Debug, Clone, Serialize)]
pub struct PaginatedResult<T> {
    /// The page of data items.
    pub data: Vec<T>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl<T> PaginatedResult<T> {
    /// Build a paginated result from data, current page info, and a total count.
    pub fn new(data: Vec<T>, page: i64, page_size: i64, total: i64) -> Self {
        Self {
            data,
            pagination: Pagination::new(page, page_size, total),
        }
    }
}

// ---------------------------------------------------------------------------
// Query-string parsing
// ---------------------------------------------------------------------------

/// Reserved parameter names that are never treated as filters.
const RESERVED_PARAMS: &[&str] = &["page", "page_size", "pageSize", "per_page", "sort", "order"];

/// Parse query parameters from a URL query string.
///
/// Reads:
/// - `page` — 1-based page number (default 1, min 1).
/// - `page_size` / `pageSize` / `per_page` — items per page (default from
///   config, clamped to `1..=max_page_size`).
/// - `sort` — column to sort by (must be in `allowed_sorts` when non-empty).
/// - `order` — `asc` or `desc` (default `asc`).
/// - All other keys are treated as filters (must be in `allowed_filters` when
///   non-empty).
pub fn parse_query_string(query: &str, config: &QueryConfig) -> QueryParams {
    let pairs = parse_pairs(query);

    let page = pairs
        .get("page")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1)
        .max(1);

    let raw_page_size = pairs
        .get("page_size")
        .or_else(|| pairs.get("pageSize"))
        .or_else(|| pairs.get("per_page"))
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(config.default_page_size);
    let page_size = raw_page_size.clamp(1, config.max_page_size);

    let sort_by = pairs.get("sort").and_then(|v| {
        let v = v.trim();
        if v.is_empty() {
            return None;
        }
        if config.allowed_sorts.is_empty() || config.allowed_sorts.iter().any(|s| s == v) {
            Some(v.to_owned())
        } else {
            None
        }
    });

    let sort_order = pairs
        .get("order")
        .map(|v| match v.to_ascii_lowercase().as_str() {
            "desc" => SortOrder::Desc,
            _ => SortOrder::Asc,
        })
        .unwrap_or_default();

    let filters: HashMap<String, String> = pairs
        .into_iter()
        .filter(|(k, _)| !RESERVED_PARAMS.contains(&k.as_str()))
        .filter(|(k, _)| {
            config.allowed_filters.is_empty() || config.allowed_filters.iter().any(|f| f == k)
        })
        .collect();

    QueryParams {
        page,
        page_size,
        sort_by,
        sort_order,
        filters,
    }
}

/// Minimal query-string parser: splits on `&`, then on `=`.
fn parse_pairs(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.trim();
            let value = parts.next().unwrap_or("").trim();
            if key.is_empty() {
                None
            } else {
                Some((key.to_owned(), value.to_owned()))
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> QueryConfig {
        QueryConfig::default()
    }

    // -- basic parsing -------------------------------------------------------

    #[test]
    fn parse_basic_page_and_page_size() {
        let params = parse_query_string("page=2&page_size=10", &default_config());
        assert_eq!(params.page, 2);
        assert_eq!(params.page_size, 10);
    }

    #[test]
    fn parse_page_size_alias_camel_case() {
        let params = parse_query_string("pageSize=15", &default_config());
        assert_eq!(params.page_size, 15);
    }

    #[test]
    fn parse_page_size_alias_per_page() {
        let params = parse_query_string("per_page=25", &default_config());
        assert_eq!(params.page_size, 25);
    }

    // -- clamping ------------------------------------------------------------

    #[test]
    fn clamp_page_size_to_max() {
        let config = QueryConfig {
            max_page_size: 50,
            ..default_config()
        };
        let params = parse_query_string("page_size=200", &config);
        assert_eq!(params.page_size, 50);
    }

    #[test]
    fn clamp_page_size_minimum_to_one() {
        let params = parse_query_string("page_size=0", &default_config());
        assert_eq!(params.page_size, 1);
    }

    #[test]
    fn clamp_page_min_to_one() {
        let params = parse_query_string("page=0", &default_config());
        assert_eq!(params.page, 1);
    }

    #[test]
    fn negative_page_clamps_to_one() {
        let params = parse_query_string("page=-5", &default_config());
        assert_eq!(params.page, 1);
    }

    // -- defaults ------------------------------------------------------------

    #[test]
    fn defaults_when_empty_query() {
        let params = parse_query_string("", &default_config());
        assert_eq!(params.page, 1);
        assert_eq!(params.page_size, 20);
        assert!(params.sort_by.is_none());
        assert_eq!(params.sort_order, SortOrder::Asc);
        assert!(params.filters.is_empty());
    }

    #[test]
    fn defaults_when_no_params_provided() {
        let params = parse_query_string("unrelated=foo", &default_config());
        assert_eq!(params.page, 1);
        assert_eq!(params.page_size, 20);
    }

    // -- sort ----------------------------------------------------------------

    #[test]
    fn parse_sort_and_order() {
        let params = parse_query_string("sort=name&order=desc", &default_config());
        assert_eq!(params.sort_by.as_deref(), Some("name"));
        assert_eq!(params.sort_order, SortOrder::Desc);
    }

    #[test]
    fn parse_sort_defaults_to_asc() {
        let params = parse_query_string("sort=created_at", &default_config());
        assert_eq!(params.sort_by.as_deref(), Some("created_at"));
        assert_eq!(params.sort_order, SortOrder::Asc);
    }

    #[test]
    fn sort_rejected_when_not_in_allowed_sorts() {
        let config = QueryConfig {
            allowed_sorts: vec!["name".into(), "created_at".into()],
            ..default_config()
        };
        let params = parse_query_string("sort=email", &config);
        assert!(params.sort_by.is_none());
    }

    #[test]
    fn sort_accepted_when_in_allowed_sorts() {
        let config = QueryConfig {
            allowed_sorts: vec!["name".into()],
            ..default_config()
        };
        let params = parse_query_string("sort=name", &config);
        assert_eq!(params.sort_by.as_deref(), Some("name"));
    }

    // -- filters -------------------------------------------------------------

    #[test]
    fn parse_filters() {
        let params = parse_query_string("status=active&type=premium", &default_config());
        assert_eq!(params.filters.get("status").unwrap(), "active");
        assert_eq!(params.filters.get("type").unwrap(), "premium");
    }

    #[test]
    fn allowed_filters_enforcement() {
        let config = QueryConfig {
            allowed_filters: vec!["status".into()],
            ..default_config()
        };
        let params = parse_query_string("status=active&type=premium", &config);
        assert_eq!(params.filters.get("status").unwrap(), "active");
        assert!(!params.filters.contains_key("type"));
    }

    #[test]
    fn reserved_params_not_treated_as_filters() {
        let params = parse_query_string(
            "page=1&page_size=10&sort=name&order=asc&status=active",
            &default_config(),
        );
        assert!(!params.filters.contains_key("page"));
        assert!(!params.filters.contains_key("page_size"));
        assert!(!params.filters.contains_key("sort"));
        assert!(!params.filters.contains_key("order"));
        assert_eq!(params.filters.get("status").unwrap(), "active");
    }

    // -- limit / offset helpers ----------------------------------------------

    #[test]
    fn limit_and_offset() {
        let params = parse_query_string("page=3&page_size=10", &default_config());
        assert_eq!(params.limit(), 10);
        assert_eq!(params.offset(), 20);
    }

    #[test]
    fn offset_is_zero_for_first_page() {
        let params = parse_query_string("page=1&page_size=25", &default_config());
        assert_eq!(params.offset(), 0);
    }

    // -- to_find_opts --------------------------------------------------------

    #[test]
    fn to_find_opts_basic() {
        let params = parse_query_string(
            "page=2&page_size=10&sort=name&order=desc&status=active",
            &default_config(),
        );
        let opts = params.to_find_opts();
        assert_eq!(opts.limit, Some(10));
        assert_eq!(opts.offset, Some(10));
        assert_eq!(opts.order_by, vec!["name DESC"]);
        assert!(opts.filters.iter().any(|(k, _)| k == "status"));
    }

    // -- Pagination ----------------------------------------------------------

    #[test]
    fn pagination_math() {
        let p = Pagination::new(1, 10, 95);
        assert_eq!(p.total_pages, 10);
    }

    #[test]
    fn pagination_exact_division() {
        let p = Pagination::new(1, 10, 100);
        assert_eq!(p.total_pages, 10);
    }

    #[test]
    fn pagination_zero_total() {
        let p = Pagination::new(1, 10, 0);
        assert_eq!(p.total_pages, 0);
    }

    #[test]
    fn pagination_single_item() {
        let p = Pagination::new(1, 10, 1);
        assert_eq!(p.total_pages, 1);
    }

    // -- PaginatedResult -----------------------------------------------------

    #[test]
    fn paginated_result_construction() {
        let result = PaginatedResult::new(vec!["a", "b", "c"], 2, 10, 25);
        assert_eq!(result.data.len(), 3);
        assert_eq!(result.pagination.page, 2);
        assert_eq!(result.pagination.page_size, 10);
        assert_eq!(result.pagination.total, 25);
        assert_eq!(result.pagination.total_pages, 3);
    }

    // -- SortOrder -----------------------------------------------------------

    #[test]
    fn sort_order_sql_representation() {
        assert_eq!(SortOrder::Asc.as_sql(), "ASC");
        assert_eq!(SortOrder::Desc.as_sql(), "DESC");
    }

    #[test]
    fn sort_order_display() {
        assert_eq!(format!("{}", SortOrder::Asc), "ASC");
        assert_eq!(format!("{}", SortOrder::Desc), "DESC");
    }
}
