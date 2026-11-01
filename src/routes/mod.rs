//! The mutation routes, grouped by domain. Each takes the request context and its JSON payload
//! and returns the reply; the registry is `api::find_route`.

pub mod accounts_routes;
pub mod asset_routes;
pub mod finding_routes;
pub mod inspection_routes;
pub mod location_routes;
pub mod setup_routes;
pub mod work_routes;
