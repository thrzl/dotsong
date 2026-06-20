use last_fm_rs::Client;

pub use last_fm_rs::AuthToken;

pub const LASTFM_API_KEY: &str = env!("LASTFM_API_KEY");
pub const LASTFM_API_SECRET: &str = env!("LASTFM_API_SECRET");
pub const LIBREFM_API_KEY: &str = env!("LIBREFM_API_KEY");
pub const LIBREFM_API_SECRET: &str = env!("LIBREFM_API_SECRET");

const LIBREFM_API_BASE: &str = "https://libre.fm/2.0/";
const LIBREFM_AUTH_URL: &str = "https://www.libre.fm/api/auth/";

pub async fn fetch_token() -> Result<AuthToken, String> {
    let client = Client::new(LASTFM_API_KEY, LASTFM_API_SECRET);
    client
        .get_token()
        .await
        .map_err(|e| format!("last.fm error: {e}"))
}

pub fn build_auth_url(token: &AuthToken) -> String {
    format!(
        "https://www.last.fm/api/auth/?api_key={}&token={}",
        LASTFM_API_KEY, token.token
    )
}

pub async fn exchange_token(token: &AuthToken) -> Result<String, String> {
    let client = Client::new(LASTFM_API_KEY, LASTFM_API_SECRET);
    let session = client
        .get_session(token)
        .await
        .map_err(|e| format!("last.fm error: {e}"))?;
    Ok(session.key)
}

pub async fn fetch_librefm_token() -> Result<(AuthToken, String), String> {
    let client = Client::new(LIBREFM_API_KEY, LIBREFM_API_SECRET)
        .with_api_base(LIBREFM_API_BASE)
        .map_err(|e| format!("libre.fm error: {e}"))?
        .with_auth_url(LIBREFM_AUTH_URL)
        .map_err(|e| format!("libre.fm error: {e}"))?;
    let token = client
        .get_token()
        .await
        .map_err(|e| format!("libre.fm error: {e}"))?;
    let auth_url = client
        .get_auth_url(&token)
        .map_err(|e| format!("libre.fm error: {e}"))?;
    Ok((token, auth_url))
}

pub async fn exchange_librefm_token(token: &AuthToken) -> Result<String, String> {
    let client = Client::new(LIBREFM_API_KEY, LIBREFM_API_SECRET)
        .with_api_base(LIBREFM_API_BASE)
        .map_err(|e| format!("libre.fm error: {e}"))?;
    let session = client
        .get_session(token)
        .await
        .map_err(|e| format!("libre.fm error: {e}"))?;
    Ok(session.key)
}
