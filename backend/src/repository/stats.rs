use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::Result;

/// Width of one step of the stats series.
#[derive(Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum StatsInterval {
    Hour,
    Day,
    Week,
    Month,
    Year,
}

impl StatsInterval {
    /// Postgres `date_trunc` field, also a valid interval unit.
    fn as_sql(self) -> &'static str {
        match self {
            StatsInterval::Hour => "hour",
            StatsInterval::Day => "day",
            StatsInterval::Week => "week",
            StatsInterval::Month => "month",
            StatsInterval::Year => "year",
        }
    }
}

/// How far back the stats series starts.
#[derive(Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StatsPeriod {
    LastWeek,
    LastMonth,
    LastYear,
    /// From the first upload.
    AllTime,
}

impl StatsPeriod {
    /// Postgres interval back from now; `None` starts at the first upload.
    fn as_sql(self) -> Option<&'static str> {
        match self {
            StatsPeriod::LastWeek => Some("7 days"),
            StatsPeriod::LastMonth => Some("30 days"),
            StatsPeriod::LastYear => Some("1 year"),
            StatsPeriod::AllTime => None,
        }
    }
}

/// Totals for one step of the stats series.
#[derive(Serialize, ToSchema)]
pub struct PeriodTotal {
    /// UTC start of the step as an ISO-8601 string.
    pub period: String,
    /// Files stored by the end of the step.
    pub files: i64,
    /// Bytes stored by the end of the step (main files, thumbnails excluded).
    pub bytes: i64,
    /// Files uploaded during the step.
    pub uploads: i64,
    /// Bytes uploaded during the step.
    pub uploaded_bytes: i64,
}

/// One row per `interval` step to now, empty steps included so the series
/// plots evenly. The series starts `period` back from now, or at the first
/// upload for `AllTime` (no rows when nothing was uploaded), rounded down to an
/// `interval` step, so it can start before the window `window_totals` counts.
/// Files are never deleted, so the files uploaded before the start plus a
/// running sum after it is the stored total.
pub async fn period_totals(
    pool: &PgPool,
    period: StatsPeriod,
    interval: StatsInterval,
) -> Result<Vec<PeriodTotal>> {
    Ok(sqlx::query_as!(
        PeriodTotal,
        "with start as ( \
           select date_trunc($1, coalesce(now() - $2::text::interval, \
                                          (select min(created_at) from files)) \
                                 at time zone 'UTC') as at), \
         per_step as ( \
           select date_trunc($1, created_at at time zone 'UTC') as step, \
                  count(*) as files, sum(size) as bytes \
           from files, start where created_at at time zone 'UTC' >= start.at \
           group by 1), \
         earlier as ( \
           select count(*) as files, coalesce(sum(size), 0) as bytes \
           from files, start where created_at at time zone 'UTC' < start.at) \
         select to_char(step, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as \"period!\", \
                (earlier.files + sum(coalesce(per_step.files, 0)) over (order by step))::bigint \
                  as \"files!\", \
                (earlier.bytes + sum(coalesce(per_step.bytes, 0)) over (order by step))::bigint \
                  as \"bytes!\", \
                coalesce(per_step.files, 0) as \"uploads!\", \
                coalesce(per_step.bytes, 0)::bigint as \"uploaded_bytes!\" \
         from earlier, start, \
              generate_series(start.at, now() at time zone 'UTC', ('1 ' || $1)::interval) as step \
         left join per_step using (step) \
         order by step",
        interval.as_sql(),
        period.as_sql(),
    )
    .fetch_all(pool)
    .await?)
}

/// File count and bytes for one stored MIME type.
#[derive(Serialize, ToSchema)]
pub struct FormatTotal {
    pub mime: String,
    pub files: i64,
    pub bytes: i64,
}

/// Stored files grouped by MIME type, largest first.
pub async fn format_totals(pool: &PgPool) -> Result<Vec<FormatTotal>> {
    Ok(sqlx::query_as!(
        FormatTotal,
        "select mime, count(*) as \"files!\", sum(size)::bigint as \"bytes!\" \
         from files group by mime order by 3 desc, mime"
    )
    .fetch_all(pool)
    .await?)
}

/// Uploads and sign-ups within `period` back from now, unrounded, unlike the
/// series which starts on an `interval` boundary.
#[derive(Serialize, ToSchema)]
pub struct WindowTotals {
    /// Users who signed up in the window; every user for `all_time`.
    pub new_users: i64,
    /// Users who uploaded in the window.
    pub active_uploaders: i64,
    /// Files uploaded in the window.
    pub uploads: i64,
    /// Bytes uploaded in the window.
    pub uploaded_bytes: i64,
}

pub async fn window_totals(pool: &PgPool, period: StatsPeriod) -> Result<WindowTotals> {
    Ok(sqlx::query_as!(
        WindowTotals,
        "select (select count(*) from users \
                 where $1::text is null or created_at >= now() - $1::text::interval) \
                  as \"new_users!\", \
                count(distinct uploaded_by_user_id) as \"active_uploaders!\", \
                count(*) as \"uploads!\", \
                coalesce(sum(size), 0)::bigint as \"uploaded_bytes!\" \
         from files where $1::text is null or created_at >= now() - $1::text::interval",
        period.as_sql(),
    )
    .fetch_one(pool)
    .await?)
}

/// One user's uploads within the window.
#[derive(Serialize, ToSchema)]
pub struct Uploader {
    pub username: String,
    pub files: i64,
    pub bytes: i64,
    /// Latest upload time as a UTC ISO-8601 string.
    pub last_upload: String,
}

/// Users who uploaded within `period` back from now, by uploaded bytes
/// descending.
// ponytail: unpaginated, page it if instances grow to many thousand uploaders
pub async fn uploaders(pool: &PgPool, period: StatsPeriod) -> Result<Vec<Uploader>> {
    Ok(sqlx::query_as!(
        Uploader,
        "select u.username, count(*) as \"files!\", sum(f.size)::bigint as \"bytes!\", \
                to_char(max(f.created_at) at time zone 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') \
                  as \"last_upload!\" \
         from files f join users u on u.id = f.uploaded_by_user_id \
         where $1::text is null or f.created_at >= now() - $1::text::interval \
         group by u.id, u.username \
         order by 3 desc, u.id",
        period.as_sql(),
    )
    .fetch_all(pool)
    .await?)
}
