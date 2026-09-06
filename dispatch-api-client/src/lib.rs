mod agent;
mod endpoint;
mod error;
mod knowledge;
mod operator;

use dispatch_types::ApiError;
pub use error::{ClientError, ClientResult};
use reqwest::StatusCode;
use serde::{Serialize, de::DeserializeOwned};

#[derive(Clone, Debug)]
pub struct DispatchClient {
    base_url: String,
    http: reqwest::Client,
    agent_id: Option<String>,
    agent_run_id: Option<i64>,
}

/// Typed access to one project's agent-facing API.
#[derive(Clone, Copy, Debug)]
pub struct ProjectClient<'a> {
    client: &'a DispatchClient,
    project: &'a str,
}

/// Typed access to one project's operator API.
#[derive(Clone, Copy, Debug)]
pub struct OperatorProjectClient<'a> {
    client: &'a DispatchClient,
    project: &'a str,
}

impl DispatchClient {
    pub fn new(base_url: impl Into<String>) -> ClientResult<Self> {
        let base_url = base_url.into();
        let parsed =
            reqwest::Url::parse(base_url.trim()).map_err(|source| ClientError::InvalidBaseUrl {
                base_url: base_url.clone(),
                reason: source.to_string(),
            })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(ClientError::InvalidBaseUrl {
                base_url,
                reason: "scheme must be http or https".to_owned(),
            });
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(ClientError::InvalidBaseUrl {
                base_url,
                reason: "query strings and fragments are not allowed".to_owned(),
            });
        }

        Ok(Self {
            base_url: parsed.as_str().trim_end_matches('/').to_owned(),
            http: reqwest::Client::new(),
            agent_id: None,
            agent_run_id: None,
        })
    }

    pub fn with_agent_context(
        mut self,
        agent_id: Option<impl Into<String>>,
        agent_run_id: Option<i64>,
    ) -> Self {
        self.agent_id = agent_id.map(Into::into);
        self.agent_run_id = agent_run_id;
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Binds subsequent agent-facing requests to one project.
    pub fn project<'a>(&'a self, project: &'a str) -> ProjectClient<'a> {
        ProjectClient {
            client: self,
            project,
        }
    }

    /// Binds subsequent operator requests to one project.
    pub fn operator_project<'a>(&'a self, project: &'a str) -> OperatorProjectClient<'a> {
        OperatorProjectClient {
            client: self,
            project,
        }
    }

    async fn get<T>(&self, endpoint: impl AsRef<str>) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        self.send(self.http.get(self.url(endpoint.as_ref()))).await
    }

    async fn post<T, B>(&self, endpoint: impl AsRef<str>, body: &B) -> ClientResult<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.send(self.http.post(self.url(endpoint.as_ref())).json(body))
            .await
    }

    async fn put<T, B>(&self, endpoint: impl AsRef<str>, body: &B) -> ClientResult<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.send(self.http.put(self.url(endpoint.as_ref())).json(body))
            .await
    }

    async fn patch<T, B>(&self, endpoint: impl AsRef<str>, body: &B) -> ClientResult<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.send(self.http.patch(self.url(endpoint.as_ref())).json(body))
            .await
    }

    async fn delete<T>(&self, endpoint: impl AsRef<str>) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        self.send(self.http.delete(self.url(endpoint.as_ref())))
            .await
    }

    async fn delete_with_body<T, B>(&self, endpoint: impl AsRef<str>, body: &B) -> ClientResult<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.send(self.http.delete(self.url(endpoint.as_ref())).json(body))
            .await
    }

    async fn delete_without_response(&self, endpoint: impl AsRef<str>) -> ClientResult<()> {
        self.send_decoded(
            self.http.delete(self.url(endpoint.as_ref())),
            decode_empty_response,
        )
        .await
    }

    async fn send<T>(&self, request: reqwest::RequestBuilder) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        self.send_decoded(request, decode_response::<T>).await
    }

    async fn send_decoded<T>(
        &self,
        request: reqwest::RequestBuilder,
        decode: impl FnOnce(StatusCode, &[u8]) -> ClientResult<T>,
    ) -> ClientResult<T> {
        let response = self
            .with_agent_context_headers(request)
            .send()
            .await
            .map_err(|source| ClientError::Request {
                base_url: self.base_url.clone(),
                source,
            })?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|source| ClientError::ResponseBody { source })?;
        decode(status, &bytes)
    }

    fn with_agent_context_headers(
        &self,
        mut request: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        if let Some(agent_id) = &self.agent_id {
            request = request.header("X-Dispatch-Agent-Id", agent_id);
        }
        if let Some(agent_run_id) = self.agent_run_id {
            request = request.header("X-Dispatch-Agent-Run-Id", agent_run_id);
        }
        request
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

impl ProjectClient<'_> {
    /// Returns the project name bound to this client.
    pub fn project_name(&self) -> &str {
        self.project
    }

    fn endpoint(&self, route: &'static str) -> endpoint::Endpoint {
        endpoint::Endpoint::project(self.project, route)
    }

    async fn get<T>(&self, endpoint: impl AsRef<str>) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        self.client.get(endpoint).await
    }

    async fn post<T, B>(&self, endpoint: impl AsRef<str>, body: &B) -> ClientResult<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.client.post(endpoint, body).await
    }

    async fn patch<T, B>(&self, endpoint: impl AsRef<str>, body: &B) -> ClientResult<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.client.patch(endpoint, body).await
    }

    async fn delete<T>(&self, endpoint: impl AsRef<str>) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        self.client.delete(endpoint).await
    }
}

impl OperatorProjectClient<'_> {
    fn endpoint(&self, route: &'static str) -> endpoint::Endpoint {
        endpoint::Endpoint::operator_project(self.project, route)
    }

    async fn get<T>(&self, endpoint: impl AsRef<str>) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        self.client.get(endpoint).await
    }

    async fn post<T, B>(&self, endpoint: impl AsRef<str>, body: &B) -> ClientResult<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.client.post(endpoint, body).await
    }

    async fn put<T, B>(&self, endpoint: impl AsRef<str>, body: &B) -> ClientResult<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.client.put(endpoint, body).await
    }

    async fn delete_with_body<T, B>(&self, endpoint: impl AsRef<str>, body: &B) -> ClientResult<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.client.delete_with_body(endpoint, body).await
    }

    async fn delete_without_response(&self, endpoint: impl AsRef<str>) -> ClientResult<()> {
        self.client.delete_without_response(endpoint).await
    }
}

fn decode_empty_response(status: StatusCode, bytes: &[u8]) -> ClientResult<()> {
    if status.is_success() {
        Ok(())
    } else {
        Err(decode_api_error(status, bytes))
    }
}

fn decode_response<T>(status: StatusCode, bytes: &[u8]) -> ClientResult<T>
where
    T: DeserializeOwned,
{
    if !status.is_success() {
        return Err(decode_api_error(status, bytes));
    }

    serde_json::from_slice(bytes).map_err(|source| ClientError::Decode { source })
}

fn decode_api_error(status: StatusCode, bytes: &[u8]) -> ClientError {
    match serde_json::from_slice::<ApiError>(bytes) {
        Ok(error) => ClientError::Api { status, error },
        Err(_) => ClientError::UnexpectedApiResponse {
            status,
            body: String::from_utf8_lossy(bytes).into_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use serde_json::json;

    #[test]
    fn normalizes_the_base_url_once() {
        let client = DispatchClient::new("  http://127.0.0.1:4000///  ").unwrap();

        assert_that!(&(client.base_url())).is_equal_to("http://127.0.0.1:4000");
        assert_that!(&(client.url("/api/projects")))
            .is_equal_to("http://127.0.0.1:4000/api/projects");
    }

    #[test]
    fn scopes_project_and_operator_routes_at_construction() {
        let client = DispatchClient::new("http://127.0.0.1:4000").unwrap();
        let project = client.project("docs/tools");
        let operator = client.operator_project("docs/tools");

        assert_that!(&(project.project_name())).is_equal_to("docs/tools");
        assert_that!(&(project.endpoint("items").as_ref()))
            .is_equal_to("/api/projects/docs%2Ftools/items");
        assert_that!(&(operator.endpoint("automation/rules").as_ref()))
            .is_equal_to("/operator/api/projects/docs%2Ftools/automation/rules");
    }

    #[test]
    fn applies_agent_attribution_at_the_transport_boundary() {
        let client = DispatchClient::new("http://127.0.0.1:4000")
            .unwrap()
            .with_agent_context(Some("dispatch-run-17"), Some(17));
        let request = client
            .with_agent_context_headers(client.http.get(client.url("/api/projects")))
            .build()
            .unwrap();
        let agent_id = request
            .headers()
            .get("X-Dispatch-Agent-Id")
            .unwrap()
            .to_str()
            .unwrap();
        let agent_run_id = request
            .headers()
            .get("X-Dispatch-Agent-Run-Id")
            .unwrap()
            .to_str()
            .unwrap();

        assert_that!(&(agent_id)).is_equal_to("dispatch-run-17");
        assert_that!(&(agent_run_id)).is_equal_to("17");
    }

    #[test]
    fn rejects_invalid_or_ambiguous_base_urls_at_construction() {
        for base_url in [
            "",
            "localhost:4000",
            "file:///tmp/dispatch",
            "http://localhost:4000?project=demo",
            "http://localhost:4000/#fragment",
        ] {
            let error = DispatchClient::new(base_url).unwrap_err();
            assert_that!(&(error.to_string().contains("invalid Dispatch API URL"))).is_true();
        }
    }

    #[test]
    fn preserves_structured_api_errors_for_callers() {
        let error = decode_response::<serde_json::Value>(
            StatusCode::CONFLICT,
            br#"{"error":"stale revision","code":"stale_revision","details":{"expected":7}}"#,
        )
        .unwrap_err();

        assert_that!(&(error.status())).is_equal_to(Some(StatusCode::CONFLICT));
        let api_error = error.api_error().unwrap();
        assert_that!(&(api_error.error.as_str())).is_equal_to("stale revision");
        assert_that!(&(api_error.code.as_deref())).is_equal_to(Some("stale_revision"));
        assert_that!(&(api_error.details.as_ref())).is_equal_to(Some(&json!({ "expected": 7 })));
        assert_that!(&(error.to_string())).is_equal_to("stale revision");
        assert_that!(&(format!("{error:?}"))).is_equal_to("stale revision");
    }

    #[test]
    fn distinguishes_unstructured_api_errors_from_decode_failures() {
        let response_error =
            decode_response::<serde_json::Value>(StatusCode::BAD_GATEWAY, b"proxy failed")
                .unwrap_err();
        assert_that!(&(response_error.status())).is_equal_to(Some(StatusCode::BAD_GATEWAY));
        assert_that!(&(response_error.api_error())).is_none();
        assert_that!(&(response_error.to_string()))
            .is_equal_to("Dispatch API returned 502 Bad Gateway: proxy failed");

        let decode_error =
            decode_response::<serde_json::Value>(StatusCode::OK, b"not JSON").unwrap_err();
        assert_that!(&(decode_error.status())).is_none();
        assert_that!(&(decode_error.to_string()))
            .is_equal_to("failed to decode Dispatch API response");
    }

    #[test]
    fn empty_success_responses_do_not_require_json() {
        assert_that!(&(decode_empty_response(StatusCode::NO_CONTENT, b"").is_ok())).is_true();

        let error = decode_empty_response(
            StatusCode::CONFLICT,
            br#"{"error":"still referenced","code":"conflict"}"#,
        )
        .unwrap_err();
        assert_that!(&(error.status())).is_equal_to(Some(StatusCode::CONFLICT));
        assert_that!(&(error.api_error().unwrap().error.as_str())).is_equal_to("still referenced");
    }
}
