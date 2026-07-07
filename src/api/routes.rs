// ── API routes ──────────────────────────────────────────────
// Wires up all API endpoints under /api.
// ────────────────────────────────────────────────────────────

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .service(health)
            .service(super::auth::register)
            .service(super::auth::login)
            .service(super::auth::logout)
            .service(super::auth::me)
            .service(super::auth::create_token)
            .service(super::dropbox::oauth_start)
            .service(super::dropbox::oauth_callback)
            .service(super::dropbox::connection)
            .service(super::dropbox::update_folder)
            .service(super::dropbox::pause_connection)
            .service(super::dropbox::sync_now)
            .service(super::dropbox::delete_connection)
            .service(super::vehicles::list_vehicles)
            .service(super::vehicles::set_exact_vin_sharing)
            .service(super::vehicles::approve_exact_vin_sharing)
            .service(super::vehicles::approve_cohort_sharing)
            .service(super::vehicles::drop_vehicle)
            .service(super::ingest::upload_csv)
            .service(super::channels::list_channels)
            .service(super::dashboard::dashboard)
            .service(super::trends::trends)
            .service(super::summary::summary)
            .service(super::pairs::pairs)
            .service(super::cohort::cohort)
            .service(super::latest::latest)
            .service(super::vehicles::public_vehicle),
    );
}

#[actix_web::get("/health")]
async fn health() -> impl actix_web::Responder {
    actix_web::HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}
