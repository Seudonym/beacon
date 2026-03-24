use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};

use crate::pages::{ChatPage, LoginPage, MePage, NotFoundPage, RegisterPage};

pub fn backend_base_url() -> &'static str {
    "http://localhost:3000"
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
