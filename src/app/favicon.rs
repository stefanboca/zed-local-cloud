use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::response::{IntoResponse, Response};
use topcoat::router::{HeaderValue, header, route};

use crate::assets::ZED_LOGO_SVG;

topcoat::router::segment!(rename = "favicon.ico");

#[route(GET)]
async fn favicon(cx: &Cx) -> Result<Response> {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("image/svg+xml"),
        )],
        ZED_LOGO_SVG,
    )
        .into_response(cx)
}
