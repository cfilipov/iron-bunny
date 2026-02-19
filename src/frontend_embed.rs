use axum::{
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct FrontendAssets;

/// Serve the SvelteKit SPA from embedded assets.
///
/// Handles `/dashboard` and `/dashboard/*` paths.
/// Falls back to `index.html` for client-side routing.
pub async fn serve_frontend(uri: Uri) -> Response {
    let path = uri.path().strip_prefix("/dashboard").unwrap_or("");
    let path = path.trim_start_matches('/');

    // If path is empty, serve index.html
    let path = if path.is_empty() { "index.html" } else { path };

    match FrontendAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => {
            // SPA fallback: serve index.html for client-side routing
            if let Some(index) = FrontendAssets::get("index.html") {
                ([(header::CONTENT_TYPE, "text/html")], index.data).into_response()
            } else {
                (StatusCode::NOT_FOUND, "Frontend not built. Run: cd frontend && npm run build")
                    .into_response()
            }
        }
    }
}
