use std::{future::Future, pin::Pin, sync::Arc};

use leptos::prelude::ServerFnError;

use super::forms::RequestErrorNotifier;

pub(crate) type ServiceFuture<T> =
    Pin<Box<dyn Future<Output = Result<T, ServerFnError>> + Send + 'static>>;

#[derive(Clone)]
pub(super) struct ServiceRequest<Input, Output> {
    execute: Arc<dyn Fn(Input) -> ServiceFuture<Output> + Send + Sync>,
    request_errors: RequestErrorNotifier,
}

impl<Input, Output> ServiceRequest<Input, Output> {
    pub(super) fn new(
        execute: impl Fn(Input) -> ServiceFuture<Output> + Send + Sync + 'static,
    ) -> Self {
        Self {
            execute: Arc::new(execute),
            request_errors: RequestErrorNotifier::capture(),
        }
    }

    pub(super) async fn execute(&self, input: Input) -> Result<Output, ServerFnError> {
        let result = (self.execute)(input).await;
        if let Err(error) = &result {
            self.request_errors.show(error.to_string());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::ServiceRequest;

    #[tokio::test]
    async fn request_can_be_replaced_with_a_stateful_mock() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_request = Arc::clone(&calls);
        let request = ServiceRequest::new(move |value: usize| {
            let calls = Arc::clone(&calls_for_request);
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(value + 1)
            })
        });

        assert_eq!(request.execute(41).await.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
