use gloo_net::http::Request;
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use shared::MeResponse;
use web_sys::RequestCredentials;

use crate::app::{backend_base_url, encode_form_component};

const SESSION_USERNAME_KEY: &str = "beacon.username";

#[component]
pub fn RegisterPage() -> impl IntoView {
    let navigate = use_navigate();
    let navigate_for_session = navigate.clone();
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error_msg = RwSignal::new(Option::<String>::None);
    let loading = RwSignal::new(false);

    Effect::new(move |_| {
        let navigate = navigate_for_session.clone();

        leptos::task::spawn_local(async move {
            let me_url = format!("{}/me", backend_base_url());
            let result = Request::get(&me_url)
                .credentials(RequestCredentials::Include)
                .send()
                .await;

            if let Ok(resp) = result {
                if resp.status() == 200 {
                    if let Ok(data) = resp.json::<MeResponse>().await {
                        if let Some(window) = web_sys::window() {
                            if let Ok(Some(storage)) = window.session_storage() {
                                let _ = storage.set_item(SESSION_USERNAME_KEY, &data.username);
                            }
                        }
                    }

                    let _ = navigate("/me", Default::default());
                }
            }
        });
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        let navigate = navigate.clone();
        ev.prevent_default();

        let username_value = username.get().trim().to_string();
        let password_value = password.get();

        if username_value.is_empty() || password_value.is_empty() {
            error_msg.set(Some("Username and password are required.".into()));
            return;
        }

        let body = format!(
            "username={}&password={}",
            encode_form_component(&username_value),
            encode_form_component(&password_value)
        );
        let register_url = format!("{}/register", backend_base_url());

        loading.set(true);
        error_msg.set(None);

        leptos::task::spawn_local(async move {
            let result = Request::post(&register_url)
                .credentials(RequestCredentials::Include)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(body)
                .unwrap()
                .send()
                .await;

            loading.set(false);

            match result {
                Ok(resp) => match resp.status() {
                    200 => {
                        if let Ok(data) = resp.json::<MeResponse>().await {
                            if let Some(window) = web_sys::window() {
                                if let Ok(Some(storage)) = window.session_storage() {
                                    let _ = storage.set_item(SESSION_USERNAME_KEY, &data.username);
                                }
                            }
                        }

                        navigate("/me", Default::default())
                    }
                    400 => error_msg.set(Some("Username must be 1-32 chars and password at least 8 chars.".into())),
                    409 => error_msg.set(Some("That username is already taken.".into())),
                    _ => error_msg.set(Some("Something went wrong. Try again.".into())),
                },
                Err(_) => error_msg.set(Some("Network Error. Failed to reach server".into())),
            }
        });
    };

    view! {
        <section class="w-full max-w-sm border border-orange-500/40 bg-surface p-5 shadow-2xl">
            <p class="text-xs font-semibold uppercase tracking-widest text-orange-400">"Beacon Access"</p>
            <h1 class="mt-2 text-2xl font-semibold uppercase tracking-wide text-orange-50">"Register"</h1>
            <p class="mt-2 text-sm text-muted">"Create an account to join the chat."</p>

            <form class="mt-5 space-y-4" on:submit=on_submit>
                <label class="block">
                    <span class="mb-1 block text-xs font-medium uppercase tracking-wider text-muted">"Username"</span>
                    <input
                        class="w-full border border-orange-950 bg-surface-strong px-3 py-2.5 text-sm text-foreground outline-none transition focus:border-orange-400"
                        id="register-username"
                        type="text"
                        placeholder="Pick a username"
                        maxlength="32"
                        on:input=move |ev| username.set(event_target_value(&ev))
                        prop:value=username
                    />
                </label>

                <label class="block">
                    <span class="mb-1 block text-xs font-medium uppercase tracking-wider text-muted">"Password"</span>
                    <input
                        class="w-full border border-orange-950 bg-surface-strong px-3 py-2.5 text-sm text-foreground outline-none transition focus:border-orange-400"
                        id="register-password"
                        type="password"
                        placeholder="At least 8 characters"
                        on:input=move |ev| password.set(event_target_value(&ev))
                        prop:value=password
                    />
                </label>

                {move || {
                    error_msg
                        .get()
                        .map(|e| {
                            view! {
                                <p class="border border-red-600/50 bg-red-950/40 px-3 py-2 text-sm text-red-200">
                                    {e}
                                </p>
                            }
                        })
                }}

                <button
                    class="w-full border border-orange-500 bg-orange-500 px-3 py-2.5 text-sm font-semibold uppercase tracking-wider text-black transition hover:bg-orange-400 disabled:cursor-not-allowed disabled:opacity-60"
                    type="submit"
                    disabled=move || loading.get()
                >
                    {move || if loading.get() { "Creating account..." } else { "Register" }}
                </button>
            </form>

            <p class="mt-4 text-sm text-muted">
                "Already have an account? "
                <a class="text-orange-300 underline underline-offset-4 hover:text-orange-200" href="/login">
                    "Login"
                </a>
            </p>
        </section>
    }
}
