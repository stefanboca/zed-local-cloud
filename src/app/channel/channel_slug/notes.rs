use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::error::rewrite;
use topcoat::router::{Body, href, page, path_param};

#[page]
pub(super) async fn notes(cx: &Cx) -> Result<()> {
    let slug = path_param::<super::ChannelSlug>(cx);
    Err(rewrite(
        href!(super::channel, super::ChannelSlug(slug))
            .relative()
            .resolve(cx),
        Body::empty(),
    )
    .into())
}
