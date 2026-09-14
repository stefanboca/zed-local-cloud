use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::error::not_found;
use topcoat::router::{page, path_param};
use topcoat::view::{PromotedStr, View, view};

use crate::assets::ZED_LOGO;

topcoat::router::path_param!(channel_slug);

#[page]
async fn channel(cx: &Cx) -> Result<impl View> {
    launch_page(cx, false).await
}

#[page("./notes")]
async fn notes(cx: &Cx) -> Result<impl View> {
    launch_page(cx, true).await
}

async fn launch_page(cx: &Cx, is_notes: bool) -> Result<impl View> {
    let slug = path_param::<ChannelSlug>(cx);

    if !valid_channel_slug(slug) {
        return Err(not_found().into());
    }

    let suffix = if is_notes { "/notes" } else { "" };
    let deep_link = format!("zed://channel/{slug}{suffix}");
    Ok(view! {
        cx =>
        <main
            class="w-full max-w-md rounded-xl border border-zinc-700 bg-zinc-800 p-8 text-center shadow-xl"
        >
            <img class="mx-auto size-16 rounded-xl" src=(ZED_LOGO) alt="Zed">
            <h1 class="mt-6 text-3xl font-semibold">"Launching Zed…"</h1>
            <p class="mt-4 leading-6 text-zinc-300">
                "If Zed did not open, use the button below."
            </p>
            <a
                class="mt-6 inline-flex rounded-lg bg-violet-600 px-4 py-2 font-semibold text-white hover:bg-violet-700"
                href=(deep_link.clone())
            >
                (if is_notes {
                    PromotedStr(&"Open channel notes")
                } else {
                    PromotedStr(&"Open channel")
                })
            </a>
        </main>
        $(raw!(
            "window.location.href = ${deep_link}.toString() + window.location.hash",
            "",
        ))
    })
}

fn valid_channel_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && slug
            .rsplit('-')
            .next()
            .is_some_and(|id| id.parse::<u64>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::valid_channel_slug;

    #[test]
    fn accepts_zed_channel_slugs() {
        assert!(valid_channel_slug("test-41767"));
        assert!(valid_channel_slug("41767"));
    }

    #[test]
    fn rejects_non_channel_slugs() {
        assert!(!valid_channel_slug("test"));
        assert!(!valid_channel_slug("test-"));
        assert!(!valid_channel_slug("test/41767"));
    }
}
