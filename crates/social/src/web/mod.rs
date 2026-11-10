//! The public website: landing page, privacy policy and terms, plus the
//! sign-in pages (see `auth`). It is served by the main server itself, so it
//! ships and updates together with the API and needs no other infrastructure.

pub(crate) mod i18n;
pub(crate) mod pages;

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::state::AppState;
use i18n::{negotiate, Lang};

const STYLE: &str = include_str!("assets/style.css");
const LOGO: &str = include_str!("assets/logo.svg");
const SOCIAL_PREVIEW: &[u8] = include_bytes!("assets/social-preview.png");

/// Only what the pages themselves load; nothing from anywhere else, and the
/// site cannot be framed.
const CSP: &str =
    "default-src 'none'; img-src 'self'; style-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

/// The page with the sign-in form may also send the browser on to the app's
/// loopback listener: browsers apply `form-action` to the redirect that follows
/// a form post, too.
const CSP_SIGN_IN_FORM: &str = "default-src 'none'; img-src 'self'; style-src 'self'; base-uri 'none'; form-action 'self' http://127.0.0.1:*; frame-ancestors 'none'";

#[derive(Debug, Deserialize)]
struct LangQuery {
    lang: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(landing))
        .route("/privacy", get(privacy))
        .route("/terms", get(terms))
        .route("/robots.txt", get(robots))
        .route("/sitemap.xml", get(sitemap))
        .route("/assets/style.css", get(style))
        .route("/assets/logo.svg", get(logo))
        .route("/favicon.svg", get(logo))
        .route("/assets/social-preview.png", get(social_preview))
}

fn lang_of(query: &LangQuery, headers: &HeaderMap) -> Lang {
    negotiate(
        query.lang.as_deref(),
        headers.get(header::ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()),
    )
}

pub(crate) fn html(body: String, lang: Lang) -> Response {
    html_with_csp(body, lang, CSP)
}

/// A page whose form posts to the server and ends at the app's loopback address.
pub(crate) fn html_sign_in_form(body: String, lang: Lang) -> Response {
    html_with_csp(body, lang, CSP_SIGN_IN_FORM)
}

fn html_with_csp(body: String, lang: Lang, csp: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=300"),
            (header::CONTENT_SECURITY_POLICY, csp),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
            // Which language was served depends on the request.
            (header::VARY, "Accept-Language"),
        ],
        [(header::CONTENT_LANGUAGE, lang.code())],
        body,
    )
        .into_response()
}

pub(crate) fn base(state: &AppState) -> &str {
    state.settings.web_base_url.trim_end_matches('/')
}

async fn landing(State(state): State<AppState>, Query(q): Query<LangQuery>, headers: HeaderMap) -> Response {
    let lang = lang_of(&q, &headers);
    html(pages::landing(base(&state), lang), lang)
}

async fn privacy(State(state): State<AppState>, Query(q): Query<LangQuery>, headers: HeaderMap) -> Response {
    let lang = lang_of(&q, &headers);
    html(pages::privacy(base(&state), lang), lang)
}

async fn terms(State(state): State<AppState>, Query(q): Query<LangQuery>, headers: HeaderMap) -> Response {
    let lang = lang_of(&q, &headers);
    html(pages::terms(base(&state), lang), lang)
}

async fn robots(State(state): State<AppState>) -> Response {
    // The sign-in pages are for people, not for the index.
    let text = format!(
        "User-agent: *\nAllow: /\nDisallow: /auth/\n\nSitemap: {}/sitemap.xml\n",
        base(&state)
    );
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        text,
    )
        .into_response()
}

async fn sitemap(State(state): State<AppState>) -> Response {
    let base = base(&state);
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
    );
    for path in ["/", "/privacy", "/terms"] {
        xml.push_str(&format!(
            "<url><loc>{base}{es}</loc><xhtml:link rel=\"alternate\" hreflang=\"es\" href=\"{base}{es}\"/><xhtml:link rel=\"alternate\" hreflang=\"en\" href=\"{base}{en}\"/></url>\n",
            es = pages::href(path, Lang::Es),
            en = pages::href(path, Lang::En).replace('&', "&amp;"),
        ));
    }
    xml.push_str("</urlset>\n");
    (
        [
            (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        xml,
    )
        .into_response()
}

fn asset(content_type: &'static str, body: impl Into<axum::body::Body>) -> Response {
    let mut response = Response::new(body.into());
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    *response.status_mut() = StatusCode::OK;
    response
}

async fn style() -> Response {
    asset("text/css; charset=utf-8", STYLE)
}

async fn logo() -> Response {
    asset("image/svg+xml", LOGO)
}

async fn social_preview() -> Response {
    asset("image/png", SOCIAL_PREVIEW)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_files_are_what_they_say() {
        assert!(STYLE.contains("--gold"));
        assert!(LOGO.trim_start().starts_with("<svg"));
        assert_eq!(&SOCIAL_PREVIEW[..8], b"\x89PNG\r\n\x1a\n");
    }
}
