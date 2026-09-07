pub(crate) mod origin;
pub(crate) mod request;
use self::request::{ServiceFuture, ServiceRequest};
use std::future::Future;

#[cfg(not(feature = "ssr"))]
use leptonic::components::prelude::{Toast, ToastTimeout, ToastVariant, Toasts};
use leptos::prelude::ServerFnError;
#[cfg(not(feature = "ssr"))]
use leptos::prelude::*;
#[cfg(not(feature = "ssr"))]
use time::OffsetDateTime;
#[cfg(not(feature = "ssr"))]
use uuid::Uuid;

#[derive(Clone, Default)]
pub(crate) struct HttpService {
    #[cfg(not(feature = "ssr"))]
    toasts: Option<Toasts>,
}

impl HttpService {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(not(feature = "ssr"))]
            toasts: use_context::<Toasts>(),
        }
    }

    pub(crate) fn show(&self, message: String) {
        #[cfg(not(feature = "ssr"))]
        if let Some(toasts) = &self.toasts {
            let body = message;
            toasts.push(Toast {
                id: Uuid::new_v4(),
                created_at: OffsetDateTime::now_utc(),
                variant: ToastVariant::Error,
                header: ViewFn::from(|| "Request failed"),
                body: ViewFn::from(move || body.clone()),
                timeout: ToastTimeout::DefaultDelay,
            });
        }

        #[cfg(feature = "ssr")]
        let _ = message;
    }
}

impl HttpService {
    pub(crate) fn get() -> Self {
        leptos::prelude::expect_context()
    }

    pub(crate) fn api_base_url(&self) -> String {
        self::origin::api_base_url()
    }

    /// Typed transport for custom browser endpoints that are not Leptos server functions.
    pub(crate) async fn post_empty<O: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<O, ServerFnError> {
        #[cfg(not(feature = "ssr"))]
        {
            self.execute(async {
                let response = gloo_net::http::Request::post(path)
                    .send()
                    .await
                    .map_err(|error| ServerFnError::new(error.to_string()))?;
                if !response.ok() {
                    return Err(ServerFnError::new(format!(
                        "Request failed (HTTP {})",
                        response.status()
                    )));
                }
                response
                    .json::<O>()
                    .await
                    .map_err(|error| ServerFnError::new(error.to_string()))
            })
            .await
        }
        #[cfg(feature = "ssr")]
        {
            let _ = path;
            Err(ServerFnError::new("This operation requires the browser."))
        }
    }

    pub(crate) fn request<I, O>(
        &self,
        execute: impl Fn(I) -> ServiceFuture<O> + Send + Sync + 'static,
    ) -> ServiceRequest<I, O> {
        ServiceRequest::new(self.clone(), execute)
    }

    pub(crate) async fn execute<O>(
        &self,
        request: impl Future<Output = Result<O, ServerFnError>>,
    ) -> Result<O, ServerFnError> {
        let result = request.await;
        if let Err(error) = &result {
            self.show(error.to_string());
        }
        result
    }
}
