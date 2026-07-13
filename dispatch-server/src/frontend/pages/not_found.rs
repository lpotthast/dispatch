use super::error::error_content;
use leptos::prelude::*;
#[cfg(feature = "ssr")]
use leptos_axum::{ResponseOptions, ResponseParts};
use leptos_meta::Title;

#[component]
pub fn PageErr404() -> impl IntoView {
    #[cfg(feature = "ssr")]
    if let Some(options) = use_context::<ResponseOptions>() {
        options.overwrite(ResponseParts {
            status: Some(axum::http::StatusCode::NOT_FOUND),
            ..Default::default()
        });
    }

    view! {
        <Title text="Not found"/>
        {error_content("Page not found.".to_owned())}
    }
}
