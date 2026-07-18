use browser_test::thirtyfour::WebDriver;
use leptos_browser_test::{Report, ResultExt, bail};

pub(crate) async fn browser_request(
    driver: &WebDriver,
    method: reqwest::Method,
    path: &str,
) -> Result<reqwest::RequestBuilder, Report> {
    let url = driver
        .current_url()
        .await
        .context("failed to read browser URL for setup request")?
        .join(path)
        .context_with(|| format!("failed to resolve browser-test request path {path:?}"))?;
    Ok(reqwest::Client::new().request(method, url))
}

pub(crate) async fn response_text(
    response: reqwest::Response,
    operation: &str,
) -> Result<String, Report> {
    let status = response.status();
    let body = response
        .text()
        .await
        .context_with(|| format!("failed to read {operation} response body"))?;
    if !status.is_success() {
        bail!("{operation} failed with {status}: {body}");
    }
    Ok(body)
}
