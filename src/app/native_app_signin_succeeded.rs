use topcoat::{
    Result,
    router::page,
    view::{View, view},
};

topcoat::router::segment!(rename = "native_app_signin_succeeded");

#[page]
pub(crate) async fn sign_in_succeeded() -> Result<impl View> {
    Ok(view! {
        <main class="w-full max-w-md rounded-xl border border-zinc-700 bg-zinc-800 p-8">
            <h1 class="text-2xl font-semibold">"Authentication Successful"</h1>
            <p class="mt-4 leading-6 text-zinc-300">
                "You have successfully signed into Zed and can close this page at any time."
            </p>
        </main>
    })
}
