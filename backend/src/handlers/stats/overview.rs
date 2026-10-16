use actix_web::{
    HttpResponse,
    web::{Data, Query},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::repository::stats::{
    self, FormatTotal, PeriodTotal, StatsInterval, StatsPeriod, Uploader, WindowTotals,
};

/// Storage overview for the admin dashboard.
#[derive(Serialize, ToSchema)]
pub struct StatsResponse {
    pub window: WindowTotals,
    /// Totals per `interval` step, from the step holding the start of
    /// `period` to now.
    pub periods: Vec<PeriodTotal>,
    /// Stored files per MIME type, largest first.
    pub formats: Vec<FormatTotal>,
    /// Users who uploaded within `period`, by uploaded bytes descending.
    pub uploaders: Vec<Uploader>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StatsQuery {
    pub period: StatsPeriod,
    pub interval: StatsInterval,
}

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/stats/overview",
    params(StatsQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Storage overview for the admin dashboard", body = StatsResponse),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Authenticated but not an admin"),
    )
)]
pub async fn stats_overview(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    query: Query<StatsQuery>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let pool = pool.get_ref();
    let (window, periods, formats, uploaders) = tokio::try_join!(
        stats::window_totals(pool, query.period),
        stats::period_totals(pool, query.period, query.interval),
        stats::format_totals(pool),
        stats::uploaders(pool, query.period),
    )?;
    Ok(HttpResponse::Ok().json(StatsResponse {
        window,
        periods,
        formats,
        uploaders,
    }))
}
