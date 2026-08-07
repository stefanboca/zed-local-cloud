mod channel_members;
mod users;

use topcoat::{
    Result,
    router::{HeaderMap, error::unauthorized, header},
};

use crate::app::AppState;

pub(super) fn validate_internal_api_key(state: &AppState, headers: &HeaderMap) -> Result<()> {
    let expected = format!("Bearer {}", state.config.internal_api_key);
    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected);
    if authorized {
        Ok(())
    } else {
        Err(unauthorized().into())
    }
}
