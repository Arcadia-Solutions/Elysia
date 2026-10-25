pub mod albums;
pub mod api_keys;
pub mod chevereto_import;
pub mod files;
pub mod reports;
pub mod sessions;
pub mod settings;
pub mod stats;
pub mod users;

use std::num::NonZeroU32;

use serde::{Deserialize, Deserializer, Serialize, de::Error};
use utoipa::ToSchema;

/// Largest page size a list endpoint accepts.
pub const MAX_PAGE_SIZE: u32 = 500;

/// One page of a list, with the total across all pages.
#[derive(Serialize, ToSchema)]
pub struct PaginatedResults<T> {
    pub results: Vec<T>,
    pub page: u32,
    pub page_size: u32,
    pub total_items: i64,
}

impl<T> PaginatedResults<T> {
    pub fn new(results: Vec<T>, page: NonZeroU32, page_size: u32, total_items: i64) -> Self {
        Self {
            results,
            page: page.get(),
            page_size,
            total_items,
        }
    }
}

/// Deserialize a page size, rejecting 0 and anything above `MAX_PAGE_SIZE`.
pub fn page_size<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let size = u32::deserialize(deserializer)?;
    if (1..=MAX_PAGE_SIZE).contains(&size) {
        Ok(size)
    } else {
        Err(D::Error::custom(format!(
            "page_size must be between 1 and {MAX_PAGE_SIZE}"
        )))
    }
}
