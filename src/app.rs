use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[server]
async fn greet(name: String) -> Result<String, ServerFnError> {
    Ok(format!("Hello, {name}, from the server!"))
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Title text="remote-view"/>
        <Router>
            <main>
                <Routes fallback=|| "Not found.">
                    <Route path=StaticSegment("") view=Home/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn Home() -> impl IntoView {
    let count = RwSignal::new(0);
    let greeting = Action::new(|name: &String| greet(name.clone()));
    view! {
        <h1>"remote-view"</h1>
        <button on:click=move |_| *count.write() += 1>"Clicks: " {count}</button>
        <button on:click=move |_| { greeting.dispatch("world".to_string()); }>"Call server fn"</button>
        <p>{move || greeting.value().get().map(|r| format!("{r:?}"))}</p>
    }
}
