use utoipa::OpenApi;

use crate::handlers::upload::UploadOptionsQuery;
use crate::repository::files::{ImagesSortBy, ImagesStatus, SortOrder};
use crate::repository::stats::{StatsInterval, StatsPeriod};
use crate::repository::users::UsersSortBy;

#[derive(OpenApi)]
#[openapi(
    info(title = "Elysia image host API"),
    paths(
        crate::handlers::register::register,
        crate::handlers::login::login,
        crate::handlers::me::me,
        crate::handlers::change_password::change_password,
        crate::handlers::sso_login::sso_login,
        crate::handlers::sso_callback::sso_callback,
        crate::handlers::upload::upload,
        crate::handlers::upload_url::upload_url,
        crate::handlers::serve::serve,
        crate::handlers::serve_thumbnail::serve_thumbnail,
        crate::handlers::metadata::metadata,
        crate::handlers::images::images,
        crate::handlers::trash_images::trash_images,
        crate::handlers::empty_trash::empty_trash,
        crate::handlers::serve_trashed_thumbnail::serve_trashed_thumbnail,
        crate::handlers::stats::overview::stats_overview,
        crate::handlers::get_elysia_settings::get_elysia_settings,
        crate::handlers::get_public_elysia_settings::get_public_elysia_settings,
        crate::handlers::get_elysia_unauth_settings::get_elysia_unauth_settings,
        crate::handlers::put_elysia_settings::put_elysia_settings,
        crate::handlers::users::list_users::list_users,
        crate::handlers::users::get_user::get_user,
        crate::handlers::users::set_user_banned::set_user_banned,
    ),
    // Query parameter types are inlined into each path; list them so the
    // generated client still gets them as named types. The rest is collected
    // from the paths.
    components(schemas(UploadOptionsQuery, StatsInterval, StatsPeriod, ImagesSortBy, ImagesStatus, SortOrder, UsersSortBy))
)]
pub struct ApiDoc;
