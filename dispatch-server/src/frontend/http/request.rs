use super::HttpService;
use leptos::prelude::ServerFnError;
use std::{future::Future, pin::Pin, sync::Arc};

pub(crate) type ServiceFuture<T> =
    Pin<Box<dyn Future<Output = Result<T, ServerFnError>> + Send + 'static>>;

#[derive(Clone)]
pub(crate) struct ServiceRequest<Input, Output> {
    execute: Arc<dyn Fn(Input) -> ServiceFuture<Output> + Send + Sync>,
    http: HttpService,
}

impl<Input, Output> ServiceRequest<Input, Output> {
    pub(crate) fn new(
        http: HttpService,
        execute: impl Fn(Input) -> ServiceFuture<Output> + Send + Sync + 'static,
    ) -> Self {
        Self {
            execute: Arc::new(execute),
            http,
        }
    }

    /// Use when the calling surface renders recoverable errors alongside the user's draft.
    pub(crate) async fn execute_inline(&self, input: Input) -> Result<Output, ServerFnError> {
        (self.execute)(input).await
    }

    pub(crate) async fn execute(&self, input: Input) -> Result<Output, ServerFnError> {
        self.http.execute((self.execute)(input)).await
    }
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::ServiceRequest;

    #[tokio::test]
    async fn request_can_be_replaced_with_a_stateful_mock() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_request = Arc::clone(&calls);
        let request = ServiceRequest::new(super::HttpService::default(), move |value: usize| {
            let calls = Arc::clone(&calls_for_request);
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(value + 1)
            })
        });

        assert_that!(&(request.execute(41).await.unwrap())).is_equal_to(42);
        assert_that!(&(calls.load(Ordering::SeqCst))).is_equal_to(1);
    }
}
