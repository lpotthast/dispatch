use leptos::prelude::*;

pub(super) fn api_base_url() -> String {
    format!("{}/api", request_origin())
}

#[cfg(feature = "ssr")]
fn request_origin() -> String {
    use_context::<axum::http::request::Parts>()
        .map(|parts| request_origin_from_headers(&parts.headers))
        .unwrap_or_else(|| "http://127.0.0.1:4000".to_owned())
}

#[cfg(feature = "ssr")]
fn request_origin_from_headers(headers: &axum::http::HeaderMap) -> String {
    use axum::http::header;

    let scheme = header_value(headers, "x-forwarded-proto")
        .and_then(normalized_http_scheme)
        .unwrap_or("http");
    let host = header_value(headers, "x-forwarded-host")
        .and_then(valid_authority)
        .or_else(|| header_value(headers, header::HOST.as_str()).and_then(valid_authority))
        .unwrap_or("127.0.0.1:4000");
    format!("{scheme}://{host}")
}

#[cfg(feature = "ssr")]
fn normalized_http_scheme(value: &str) -> Option<&'static str> {
    if value.eq_ignore_ascii_case("http") {
        Some("http")
    } else if value.eq_ignore_ascii_case("https") {
        Some("https")
    } else {
        None
    }
}

#[cfg(feature = "ssr")]
fn valid_authority(value: &str) -> Option<&str> {
    value
        .parse::<axum::http::uri::Authority>()
        .ok()
        .map(|_| value)
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

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use assertr::prelude::*;
    use axum::http::{HeaderMap, HeaderValue, header};

    use super::request_origin_from_headers;

    #[test]
    fn forwarded_origin_uses_the_first_proxy_value() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https, http"));
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("dispatch.example:8443, proxy.internal"),
        );

        assert_that!(&(request_origin_from_headers(&headers)))
            .is_equal_to("https://dispatch.example:8443");
    }

    #[test]
    fn invalid_forwarded_values_fall_back_to_a_safe_origin() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", HeaderValue::from_static("javascript"));
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("dispatch.example/path"),
        );
        headers.insert(header::HOST, HeaderValue::from_static("localhost:4000"));

        assert_that!(&(request_origin_from_headers(&headers))).is_equal_to("http://localhost:4000");
    }
}
