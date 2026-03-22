use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

#[component]
pub fn NotFoundPage() -> impl IntoView {
    let navigate = use_navigate();
    let go_home = move |_| {
        let _ = navigate("/login", Default::default());
    };

    view! {
        <section class="w-full max-w-sm border border-orange-500/40 bg-surface p-5 text-center shadow-2xl">
            <p class="text-xs uppercase tracking-widest text-orange-400">"404"</p>
            <h1 class="mt-2 text-2xl font-semibold uppercase tracking-wide text-orange-50">"Page Not Found"</h1>
            <p class="mt-2 text-sm text-muted">"The route does not exist. Return to the login page."</p>
            <button
                class="mt-4 border border-orange-500 bg-orange-500 px-3 py-2 text-sm font-semibold uppercase tracking-wider text-black transition hover:bg-orange-400"
                on:click=go_home
            >
                "Go to Login"
            </button>
        </section>
    }
}
