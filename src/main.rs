use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(|| {
        view! {
            <main class="flex items-center justify-center min-h-screen bg-slate-950 text-slate-100">
                <h1 class="text-5xl font-semibold tracking-tight text-white sm:text-6xl">Beacon</h1>
            </main>
        }
    })
}
