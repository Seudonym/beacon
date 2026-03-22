use gloo_net::http::Request;
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use shared::MeResponse;
use web_sys::RequestCredentials;

use crate::app::backend_base_url;

const SESSION_USERNAME_KEY: &str = "beacon.username";

#[component]
pub fn MePage() -> impl IntoView {
    let navigate = use_navigate();
    let navigate_for_load = navigate.clone();
    let username = RwSignal::new(read_cached_username());
    let loading = RwSignal::new(true);
    let error_msg = RwSignal::new(Option::<String>::None);

    Effect::new(move |_| {
        let navigate = navigate_for_load.clone();

        leptos::task::spawn_local(async move {
            let me_url = format!("{}/me", backend_base_url());
            let response = Request::get(&me_url)
                .credentials(RequestCredentials::Include)
                .send()
                .await;

            match response {
                Ok(resp) => match resp.status() {
                    200 => match resp.json::<MeResponse>().await {
                        Ok(data) => {
                            persist_username(&data.username);
                            username.set(Some(data.username));
                            error_msg.set(None);
                        }
                        Err(_) => {
                            error_msg.set(Some("Failed to read account details.".into()));
                        }
                    },
                    401 => {
                        clear_cached_username();
                        let _ = navigate("/login", Default::default());
                    }
                    _ => error_msg.set(Some("Failed to load your session.".into())),
                },
                Err(_) => error_msg.set(Some("Network error. Failed to reach server.".into())),
            }

            loading.set(false);
        });
    });

    let navigate_for_logout = navigate.clone();
    let on_logout = move |_| {
        let navigate = navigate_for_logout.clone();
        error_msg.set(None);

        leptos::task::spawn_local(async move {
            let logout_url = format!("{}/logout", backend_base_url());
            let response = Request::post(&logout_url)
                .credentials(RequestCredentials::Include)
                .send()
                .await;

            match response {
                Ok(resp) if resp.status() == 204 => {
                    clear_cached_username();
                    let _ = navigate("/login", Default::default());
                }
                Ok(_) => error_msg.set(Some("Logout failed. Try again.".into())),
                Err(_) => error_msg.set(Some("Network error. Failed to log out.".into())),
            }
        });
    };

    view! {
        <section class="w-full max-w-md">
            <div class="border border-orange-500/40 bg-surface px-6 py-7 shadow-2xl sm:px-8">
                <p class="text-xs font-semibold uppercase tracking-widest text-orange-400">
                    "Beacon Session"
                </p>
                <h1 class="mt-3 text-3xl font-semibold uppercase tracking-wide text-orange-50 sm:text-4xl">
                    "Your Account"
                </h1>

                <div class="mt-8 border border-orange-950 bg-surface-strong p-4">
                    <p class="text-xs uppercase tracking-wider text-orange-300/80">
                        "Username"
                    </p>
                    <p class="mt-3 text-lg font-semibold text-orange-50">
                        {move || {
                            if loading.get() {
                                "Loading...".to_string()
                            } else {
                                username.get().unwrap_or_else(|| "Unknown user".to_string())
                            }
                        }}
                    </p>
                </div>

                {move || {
                    error_msg
                        .get()
                        .map(|message| {
                            view! {
                                <p class="mt-6 border border-red-600/50 bg-red-950/40 px-3 py-2 text-sm text-red-200">
                                    {message}
                                </p>
                            }
                        })
                }}

                <div class="mt-8 flex gap-3">
                    <button
                        class="flex-1 border border-orange-500 bg-orange-500 px-3 py-2.5 text-sm font-semibold uppercase tracking-wider text-black transition hover:bg-orange-400"
                        on:click=move |_| {
                            let _ = navigate("/chat/lobby", Default::default());
                        }
                        disabled=move || loading.get()
                    >
                        "Open Chat"
                    </button>

                    <button
                        class="flex-1 border border-orange-700 bg-transparent px-3 py-2.5 text-sm font-semibold uppercase tracking-wider text-orange-200 transition hover:border-orange-500 hover:bg-orange-950"
                        on:click=on_logout
                        disabled=move || loading.get()
                    >
                        "Logout"
                    </button>
                </div>
            </div>
        </section>
    }
}

fn read_cached_username() -> Option<String> {
    let window = web_sys::window()?;
    let storage = window.session_storage().ok()??;
    storage.get_item(SESSION_USERNAME_KEY).ok()?
}

fn persist_username(username: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.session_storage() {
            let _ = storage.set_item(SESSION_USERNAME_KEY, username);
        }
    }
}

fn clear_cached_username() {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.session_storage() {
            let _ = storage.remove_item(SESSION_USERNAME_KEY);
        }
    }
}
