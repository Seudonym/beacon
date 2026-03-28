use leptos::prelude::*;

#[component]
pub fn LandingPage() -> impl IntoView {
    view! {
        <section class="w-full max-w-4xl border border-orange-500/30 bg-surface/95 shadow-2xl">
            <div class="grid min-h-[32rem] gap-8 px-6 py-8 sm:px-8 sm:py-10 lg:grid-cols-[1.3fr_0.9fr] lg:items-stretch">
                <div class="flex flex-col justify-center border border-orange-950/70 bg-surface-strong/80 p-6 sm:p-8">
                    <div>
                        <h1 class="text-5xl font-semibold uppercase tracking-[0.18em] text-orange-50 sm:text-7xl">
                            "beacon"
                        </h1>
                        <p class="mt-4 max-w-xl text-base text-muted sm:text-lg">
                            "privacy focused temp chat"
                        </p>
                    </div>
                </div>

                <aside class="flex flex-col justify-between border border-orange-500/20 bg-gradient-to-b from-orange-500/10 to-transparent p-6 sm:p-8">
                    <div>
                        <p class="text-xs font-semibold uppercase tracking-[0.3em] text-orange-300">
                            "Access"
                        </p>
                        <h2 class="mt-3 text-2xl font-semibold uppercase tracking-[0.12em] text-orange-50">
                            "Enter Beacon"
                        </h2>
                        <p class="mt-3 text-sm leading-6 text-muted">
                            "Create a handle, sign in, and move straight into a temporary room without the usual clutter."
                        </p>
                    </div>

                    <div class="mt-8 space-y-3">
                        <a
                            class="block w-full border border-orange-500 bg-orange-500 px-4 py-3 text-center text-sm font-semibold uppercase tracking-[0.18em] text-black transition hover:bg-orange-400"
                            href="/register"
                        >
                            "Create Account"
                        </a>
                        <a
                            class="block w-full border border-orange-500/40 px-4 py-3 text-center text-sm font-semibold uppercase tracking-[0.18em] text-orange-100 transition hover:border-orange-300 hover:bg-orange-500/10"
                            href="/login"
                        >
                            "Login"
                        </a>
                    </div>
                </aside>
            </div>
        </section>
    }
}
