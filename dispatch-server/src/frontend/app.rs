use crate::frontend::{
    components::{WorkspaceBar, provide_workspace_dock_size},
    live_events::LiveEventsProvider,
    routes::routes,
    services::provide_frontend_services,
};
use crudkit_leptos::crud_instance_mgr::CrudInstanceMgr;
use leptonic::components::prelude::{LeptonicTheme, Root};
use leptos::prelude::LeptosOptions;
use leptos::prelude::*;
use leptos_meta::{Meta, MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::components::{Outlet, Router};

#[allow(non_snake_case)]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    provide_meta_context();

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <Meta name="description" content="Dispatch project work orchestration"/>
                <meta name="theme-color" content="#20242a"/>
                <link rel="icon" type="image/png" sizes="32x32" href="/branding/favicon-32.png"/>
                <link
                    rel="apple-touch-icon"
                    sizes="180x180"
                    href="/branding/dispatch-icon-180.png"
                />
                <Title text="Dispatch"/>
                <HydrationScripts options=options.clone()/>
                <Stylesheet id="leptos" href=options.css_path()/>
                <MetaTags/>
                <AutoReload options=options.clone()/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Root default_theme=LeptonicTheme::Light>
            <Router>
                {routes::route_tree()}
            </Router>
        </Root>
    }
}

#[component]
pub fn MainLayout() -> impl IntoView {
    provide_frontend_services();
    provide_workspace_dock_size();

    view! {
        <CrudInstanceMgr>
            <LiveEventsProvider/>
            <Outlet/>
            <WorkspaceBar/>
        </CrudInstanceMgr>
    }
}
