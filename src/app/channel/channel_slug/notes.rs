use topcoat::{Result, context::Cx, router::page};

#[page]
async fn notes(cx: &Cx) -> Result {
    super::launch_page(cx, true).await
}
