use std::{future::Future, pin::Pin, sync::Arc};

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
struct RequestErrorNotifier {
    #[cfg(not(feature = "ssr"))]
    toasts: Option<Toasts>,
}

impl RequestErrorNotifier {
    fn capture() -> Self {
        Self {
            #[cfg(not(feature = "ssr"))]
            toasts: use_context::<Toasts>(),
        }
    }

    fn show(&self, message: String) {
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
        let request = ServiceRequest::new(move |value: usize| {
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
