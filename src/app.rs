use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};

use crate::pages::{ChatPage, LandingPage, LoginPage, MePage, NotFoundPage, RegisterPage};

pub fn api_base_url() -> String {
    compile_time_var("APP_API_BASE").unwrap_or_else(|| "/api".to_string())
}

pub fn websocket_base_url() -> String {
    if let Some(url) = compile_time_var("APP_WS_BASE") {
        return url;
    }

    let origin = browser_origin().unwrap_or_default();
    let scheme = if origin.starts_with("https://") {
        "wss://"
    } else {
        "ws://"
    };

    let host = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .unwrap_or(&origin);

    format!("{scheme}{host}/ws")
}

fn browser_origin() -> Option<String> {
    let window = web_sys::window()?;
    window.location().origin().ok()
}

fn compile_time_var(name: &str) -> Option<String> {
    match name {
        "APP_API_BASE" => option_env!("APP_API_BASE").map(str::to_string),
        "APP_WS_BASE" => option_env!("APP_WS_BASE").map(str::to_string),
        _ => None,
    }
}

pub fn encode_form_component(value: &str) -> String {
    js_sys::encode_uri_component(value)
        .as_string()
        .unwrap_or_else(|| value.to_string())
}

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <main class="min-h-screen bg-background text-foreground">
                <div class="mx-auto flex min-h-screen max-w-4xl items-center justify-center px-4 py-6 sm:px-6 sm:py-8">
                    <Routes fallback=|| view! { <NotFoundPage /> }>
                        <Route path=leptos_router::path!("/") view=LandingPage />
                        <Route path=leptos_router::path!("/login") view=LoginPage />
                        <Route path=leptos_router::path!("/register") view=RegisterPage />
                        <Route path=leptos_router::path!("/me") view=MePage />
                        <Route path=leptos_router::path!("/chat/:room") view=ChatPage />
                    </Routes>
                </div>
            </main>
        </Router>
    }
}
