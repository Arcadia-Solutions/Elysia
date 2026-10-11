use std::sync::RwLock;

use actix_web::web::Data;

use crate::settings::ElysiaSettings;

/// Shared, hot-swappable elysia settings. Uploads read a snapshot; a save
/// replaces the whole value, so a change applies without a restart.
pub type SharedElysiaSettings = Data<RwLock<ElysiaSettings>>;
