#[cfg(not(feature = "ssr"))]
use leptonic::components::prelude::{Toast, ToastTimeout, ToastVariant, Toasts};
#[cfg(not(feature = "ssr"))]
use leptos::prelude::*;
#[cfg(not(feature = "ssr"))]
use time::OffsetDateTime;
#[cfg(not(feature = "ssr"))]
use uuid::Uuid;

#[derive(Clone, Default)]
pub(crate) struct RequestErrorNotifier {
    #[cfg(not(feature = "ssr"))]
    toasts: Option<Toasts>,
}

impl RequestErrorNotifier {
    pub(crate) fn capture() -> Self {
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

pub(crate) fn background_form_submit(
    reset_on_success: bool,
) -> impl Fn(leptos::ev::SubmitEvent) + Clone + 'static {
    #[cfg(not(feature = "ssr"))]
    {
        let request_errors = RequestErrorNotifier::capture();
        move |event: leptos::ev::SubmitEvent| {
            event.prevent_default();
            let Some(form) = event.current_target().or_else(|| event.target()) else {
                request_errors.show("Missing submitted form".to_owned());
                return;
            };
            let form = wasm_bindgen::JsValue::from(form);
            let request_errors = request_errors.clone();
            leptos::task::spawn_local(async move {
                if let Err(message) = submit_background_form(form, reset_on_success).await {
                    request_errors.show(message);
                }
            });
        }
    }
    #[cfg(feature = "ssr")]
    {
        let _ = reset_on_success;
        move |_event: leptos::ev::SubmitEvent| {}
    }
}

#[cfg(not(feature = "ssr"))]
async fn submit_background_form(
    form: wasm_bindgen::JsValue,
    reset_on_success: bool,
) -> Result<(), String> {
    match js_submit_background_form(form, reset_on_success).await {
        Ok(message) => {
            let message = message.as_string().unwrap_or_default();
            if message.is_empty() {
                Ok(())
            } else {
                Err(message)
            }
        }
        Err(err) => Err(js_error_message(err)),
    }
}

#[cfg(not(feature = "ssr"))]
fn js_error_message(value: wasm_bindgen::JsValue) -> String {
    value
        .as_string()
        .unwrap_or_else(|| "Request failed".to_owned())
}

#[cfg(not(feature = "ssr"))]
#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export async function dispatchSubmitBackgroundForm(form, resetOnSuccess) {
  if (!(form instanceof HTMLFormElement)) {
    return 'Missing submitted form';
  }

  const response = await fetch(form.action, {
    method: (form.method || 'POST').toUpperCase(),
    body: new URLSearchParams(new FormData(form)),
    headers: { 'x-dispatch-background': 'true' },
  });

  if (!response.ok) {
    const body = await response.text();
    return body || `${response.status} ${response.statusText}`;
  }

  if (resetOnSuccess) {
    form.reset();
  }

  return '';
}
"#)]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = dispatchSubmitBackgroundForm)]
    async fn js_submit_background_form(
        form: wasm_bindgen::JsValue,
        reset_on_success: bool,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>;
}
