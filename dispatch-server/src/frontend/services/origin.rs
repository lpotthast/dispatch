use leptos::prelude::*;

pub(super) fn api_base_url() -> String {
    format!("{}/api", request_origin())
}

#[cfg(feature = "ssr")]
fn request_origin() -> String {
    use axum::http::header;

    use_context::<axum::http::request::Parts>()
        .map(|parts| {
            let scheme = header_value(&parts.headers, "x-forwarded-proto").unwrap_or("http");
            let host = header_value(&parts.headers, "x-forwarded-host")
                .or_else(|| header_value(&parts.headers, header::HOST.as_str()))
                .unwrap_or("127.0.0.1:4000");
            format!("{scheme}://{host}")
        })
        .unwrap_or_else(|| "http://127.0.0.1:4000".to_owned())
}

#[cfg(feature = "ssr")]
fn header_value<'a>(headers: &'a axum::http::HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(not(feature = "ssr"))]
fn request_origin() -> String {
    window()
        .location()
        .origin()
        .unwrap_or_else(|_| "http://127.0.0.1:4000".to_owned())
}
