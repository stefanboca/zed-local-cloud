use topcoat::{
    Result,
    context::Cx,
    router::{
        HeaderValue, header,
        response::{IntoResponse, Response},
        route,
    },
};

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
