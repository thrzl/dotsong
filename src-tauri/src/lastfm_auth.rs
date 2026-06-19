use last_fm_rs::Client;

pub use last_fm_rs::AuthToken;

pub const API_KEY: &str = env!("LASTFM_API_KEY");
pub const API_SECRET: &str = env!("LASTFM_API_SECRET");

pub async fn fetch_token() -> Result<AuthToken, String> {
    let client = Client::new(API_KEY, API_SECRET);
    client
        .get_token()
        .await
        .map_err(|e| format!("last.fm error: {e}"))
}

pub fn build_auth_url(token: &AuthToken) -> String {
    format!(
        "https://www.last.fm/api/auth/?api_key={}&token={}",
        API_KEY, token.token
    )
}

pub async fn exchange_token(token: &AuthToken) -> Result<String, String> {
    let client = Client::new(API_KEY, API_SECRET);
    let session = client
        .get_session(token)
        .await
        .map_err(|e| format!("last.fm error: {e}"))?;
    Ok(session.key)
}
