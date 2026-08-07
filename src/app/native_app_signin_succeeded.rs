use topcoat::{Result, router::page, view::view};

topcoat::router::segment!(rename = "native_app_signin_succeeded");

#[page]
async fn sign_in_succeeded() -> Result {
    view! {
        <main class="w-full max-w-md rounded-xl border border-zinc-700 bg-zinc-800 p-8">
            <h1 class="text-2xl font-semibold">"Sign-in complete"</h1>
            <p class="mt-4 leading-6 text-zinc-300">
                "You can close this page and return to Zed."
            </p>
        </main>
    }
}
