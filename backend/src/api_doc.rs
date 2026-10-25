use utoipa::OpenApi;

use crate::handlers::images::upload::UploadOptionsQuery;
use crate::repository::albums::AlbumsSortBy;
use crate::repository::files::{ImagesSortBy, ImagesStatus, SortOrder};
use crate::repository::stats::{StatsInterval, StatsPeriod};
use crate::repository::users::UsersSortBy;

#[derive(OpenApi)]
#[openapi(
    info(title = "Elysia image host API"),
    paths(
        crate::handlers::auth::register::register,
        crate::handlers::auth::login::login,
        crate::handlers::auth::logout::logout,
        crate::handlers::auth::me::me,
        crate::handlers::auth::change_password::change_password,
        crate::handlers::api_keys::list_api_keys::list_api_keys,
        crate::handlers::api_keys::create_api_key::create_api_key,
        crate::handlers::api_keys::revoke_api_key::revoke_api_key,
        crate::handlers::auth::sso_login::sso_login,
        crate::handlers::auth::sso_callback::sso_callback,
        crate::handlers::auth::sso_register::sso_register,
        crate::handlers::images::upload::upload,
        crate::handlers::images::upload_url::upload_url,
        crate::handlers::images::serve::serve,
        crate::handlers::images::serve_thumbnail::serve_thumbnail,
        crate::handlers::images::metadata::metadata,
        crate::handlers::images::list_images::images,
        crate::handlers::images::trash_images::trash_images,
        crate::handlers::images::empty_trash::empty_trash,
        crate::handlers::images::restore_images::restore_images,
        crate::handlers::images::report_image::report_image,
        crate::handlers::reports::list_reports::list_reports,
        crate::handlers::reports::dismiss_reports::dismiss_reports,
        crate::handlers::albums::list_albums::list_albums,
        crate::handlers::albums::create_album::create_album,
        crate::handlers::albums::get_album::get_album,
        crate::handlers::albums::rename_album::rename_album,
        crate::handlers::albums::delete_album::delete_album,
        crate::handlers::albums::add_album_images::add_album_images,
        crate::handlers::albums::remove_album_images::remove_album_images,
        crate::handlers::stats::overview::stats_overview,
        crate::handlers::fixtures::generate_fixtures::generate_fixtures,
        crate::handlers::fixtures::get_fixtures_progress::get_fixtures_progress,
        crate::handlers::maintenance::scan_unreferenced_images::scan_unreferenced_images,
        crate::handlers::maintenance::get_unreferenced_progress::get_unreferenced_progress,
        crate::handlers::maintenance::list_unreferenced_images::list_unreferenced_images,
        crate::handlers::maintenance::trash_unreferenced_images::trash_unreferenced_images,
        crate::handlers::maintenance::scan_similar_images::scan_similar_images,
        crate::handlers::maintenance::get_similar_progress::get_similar_progress,
        crate::handlers::maintenance::list_similar_images::list_similar_images,
        crate::handlers::maintenance::start_chevereto_import::start_chevereto_import,
        crate::handlers::maintenance::stop_chevereto_import::stop_chevereto_import,
        crate::handlers::maintenance::get_chevereto_import_progress::get_chevereto_import_progress,
        crate::handlers::maintenance::list_chevereto_import_failures::list_chevereto_import_failures,
        crate::handlers::settings::get_elysia_settings::get_elysia_settings,
        crate::handlers::settings::get_public_elysia_settings::get_public_elysia_settings,
        crate::handlers::settings::get_elysia_unauth_settings::get_elysia_unauth_settings,
        crate::handlers::settings::put_elysia_settings::put_elysia_settings,
        crate::handlers::users::list_users::list_users,
        crate::handlers::users::get_user::get_user,
        crate::handlers::users::set_user_admin::set_user_admin,
        crate::handlers::users::set_user_banned::set_user_banned,
        crate::handlers::users::set_user_storage_quota::set_user_storage_quota,
    ),
    // Query parameter types are inlined into each path; list them so the
    // generated client still gets them as named types. The rest is collected
    // from the paths.
    components(schemas(UploadOptionsQuery, AlbumsSortBy, StatsInterval, StatsPeriod, ImagesSortBy, ImagesStatus, SortOrder, UsersSortBy))
)]
pub struct ApiDoc;
