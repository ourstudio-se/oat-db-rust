use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap, StatusCode},
};
use crate::model::UserContext;

/// Axum extractor for UserContext from request headers
/// 
/// This extractor looks for user information in request headers:
/// - X-User-Id: Required user identifier
/// - X-User-Email: Optional user email
/// - X-User-Name: Optional user display name
/// 
/// For development/testing, if no headers are present, returns a default user.
#[async_trait]
impl<S> FromRequestParts<S> for UserContext
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;
        
        // Try to extract user information from headers
        if let Some(user_id) = extract_header_value(headers, "x-user-id") {
            let user_email = extract_header_value(headers, "x-user-email");
            let user_name = extract_header_value(headers, "x-user-name");
            
            Ok(UserContext::with_details(user_id, user_email, user_name))
        } else {
            // For development: return default user if no headers present
            // In production, you might want to return an error or extract from JWT
            Ok(UserContext::default_user())
        }
    }
}

/// Extract header value as string
fn extract_header_value(headers: &HeaderMap, header_name: &str) -> Option<String> {
    headers
        .get(header_name)
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};

    #[tokio::test]
    async fn test_user_context_extraction() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-user-id"),
            HeaderValue::from_static("test-user-123"),
        );
        headers.insert(
            HeaderName::from_static("x-user-email"),
            HeaderValue::from_static("test@example.com"),
        );
        
        let user_id = extract_header_value(&headers, "x-user-id");
        let user_email = extract_header_value(&headers, "x-user-email");
        
        assert_eq!(user_id, Some("test-user-123".to_string()));
        assert_eq!(user_email, Some("test@example.com".to_string()));
    }

    #[test]
    fn test_user_context_creation() {
        let ctx = UserContext::with_details(
            "user123".to_string(),
            Some("user@example.com".to_string()),
            Some("Test User".to_string()),
        );
        
        assert_eq!(ctx.user_id, "user123");
        assert_eq!(ctx.user_email, Some("user@example.com".to_string()));
        assert_eq!(ctx.user_name, Some("Test User".to_string()));
    }
}