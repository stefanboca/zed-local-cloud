mod feedback;
mod llm_tokens;
mod system_settings;
mod users;

use anyhow::Context;
use topcoat::context::{Cx, app_context};
use topcoat::router::error::unauthorized;
use topcoat::router::request::headers;
use topcoat::router::{HeaderMap, header};
use topcoat::{Error, Result};

use crate::database::Database;

#[derive(Debug)]
pub(super) struct Credentials {
    pub user_id: i32,
    pub access_token: String,
}

pub(super) fn parse_authorization_header(headers: &HeaderMap) -> Result<Credentials> {
    let value = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(unauthorized)?
        .to_str()
        .map_err(|_| unauthorized())?;
    let mut parts = value.split_whitespace();
    let user_id = parts
        .next()
        .ok_or_else(unauthorized)?
        .parse::<i32>()
        .map_err(|_| unauthorized())?;
    if user_id <= 0 {
        return Err(unauthorized().into());
    }
    let access_token = parts
        .next()
        .filter(|token| !token.is_empty())
        .ok_or_else(unauthorized)?;
    if parts.next().is_some() {
        return Err(unauthorized().into());
    }

    Ok(Credentials {
        user_id,
        access_token: access_token.to_owned(),
    })
}

pub(super) async fn validate_cached_credentials(cx: &Cx) -> Result<Credentials> {
    let database = app_context::<Database>(cx);
    let credentials = parse_authorization_header(headers(cx))?;
    let valid = database
        .cached_token_is_valid(credentials.user_id, &credentials.access_token)
        .await
        .context("failed to validate cached credentials")
        .map_err(Error::from_anyhow)?;
    if !valid {
        return Err(unauthorized().into());
    }
    Ok(credentials)
}

#[cfg(test)]
mod tests {
    use topcoat::router::HeaderValue;

    use super::*;

    #[test]
    fn parses_zed_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("42 access-token"),
        );
        let credentials = parse_authorization_header(&headers).expect("header should parse");
        assert_eq!(credentials.user_id, 42);
        assert_eq!(credentials.access_token, "access-token");
    }

    #[test]
    fn rejects_extra_authorization_fields() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("42 access-token extra"),
        );
        assert!(parse_authorization_header(&headers).is_err());
    }
}
