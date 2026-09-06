use std::{error::Error, fmt};

use dispatch_types::ApiError;
use reqwest::StatusCode;

/// Result returned by Dispatch API client operations.
pub type ClientResult<T> = std::result::Result<T, ClientError>;

/// A transport, HTTP response, or response-decoding failure.
pub enum ClientError {
    /// The configured Dispatch API URL is not an HTTP(S) base URL.
    InvalidBaseUrl { base_url: String, reason: String },
    /// The request could not be sent to the configured Dispatch API.
    Request {
        base_url: String,
        source: reqwest::Error,
    },
    /// The response body could not be read.
    ResponseBody { source: reqwest::Error },
    /// Dispatch returned its structured error response.
    Api { status: StatusCode, error: ApiError },
    /// Dispatch returned a non-success response without a structured error body.
    UnexpectedApiResponse { status: StatusCode, body: String },
    /// A successful response did not match the endpoint's response type.
    Decode { source: serde_json::Error },
}

impl fmt::Debug for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl ClientError {
    /// Returns the HTTP status when Dispatch produced an HTTP error response.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Api { status, .. } | Self::UnexpectedApiResponse { status, .. } => Some(*status),
            Self::InvalidBaseUrl { .. }
            | Self::Request { .. }
            | Self::ResponseBody { .. }
            | Self::Decode { .. } => None,
        }
    }

    /// Returns the structured Dispatch error body when one was received.
    pub fn api_error(&self) -> Option<&ApiError> {
        match self {
            Self::Api { error, .. } => Some(error),
            Self::InvalidBaseUrl { .. }
            | Self::Request { .. }
            | Self::ResponseBody { .. }
            | Self::UnexpectedApiResponse { .. }
            | Self::Decode { .. } => None,
        }
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBaseUrl { base_url, reason } => {
                write!(formatter, "invalid Dispatch API URL '{base_url}': {reason}")
            }
            Self::Request { base_url, .. } => {
                write!(formatter, "failed to call Dispatch API at {base_url}")
            }
            Self::ResponseBody { .. } => write!(formatter, "failed to read Dispatch API response"),
            Self::Api { error, .. } => formatter.write_str(&error.error),
            Self::UnexpectedApiResponse { status, body } => {
                write!(formatter, "Dispatch API returned {status}: {body}")
            }
            Self::Decode { .. } => write!(formatter, "failed to decode Dispatch API response"),
        }
    }
}

impl Error for ClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request { source, .. } | Self::ResponseBody { source } => Some(source),
            Self::Decode { source } => Some(source),
            Self::InvalidBaseUrl { .. } | Self::Api { .. } | Self::UnexpectedApiResponse { .. } => {
                None
            }
        }
    }
}
