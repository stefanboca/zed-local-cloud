mod channel_members;
mod users;

use topcoat::Result;
use topcoat::context::{Cx, app_context};
use topcoat::router::error::unauthorized;
use topcoat::router::header;
use topcoat::router::request::headers;

use crate::config::InternalApiKey;

pub(super) fn require_internal_api(cx: &Cx) -> Result<()> {
    let key = app_context::<InternalApiKey>(cx);
    let expected = format!("Bearer {}", key.0);
    let authorized = headers(cx)
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected);
    if authorized {
        Ok(())
    } else {
        Err(unauthorized().into())
    }
}
