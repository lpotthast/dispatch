use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;

#[component]
pub fn PageError() -> impl IntoView {
    let message = use_query_map().read_untracked().get("message");

    view! {
        <Title text="Error"/>
        {error_content(message.unwrap_or_else(|| "An error occurred.".to_owned()))}
    }
}

pub(crate) fn error_content(message: String) -> AnyView {
    view! {
        <main class="error">
            <h1>"Error"</h1>
            <p>{message}</p>
            <a href="/">"Back"</a>
        </main>
    }
    .into_any()
}
