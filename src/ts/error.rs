#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Missing auth token.")]
    MissingAuthToken,

    #[error("An API error occurred.")]
    BadRequest {
        source: reqwest::Error,
        response_body: Option<String>,
    },
}
