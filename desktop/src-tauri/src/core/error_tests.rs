use super::*;

#[test]
fn errors_reach_the_frontend_as_their_display_string() {
    let json = serde_json::to_string(&AppError::InvalidInput("bad port".into())).unwrap();
    assert_eq!(json, "\"Invalid input: bad port\"");
    assert_eq!(
        serde_json::to_string(&AppError::NotImplemented).unwrap(),
        "\"Not implemented\""
    );
}

#[test]
fn io_errors_convert_and_keep_their_message() {
    let error: AppError = std::io::Error::new(std::io::ErrorKind::NotFound, "gone").into();
    assert!(matches!(error, AppError::Io(_)));
    assert_eq!(error.to_string(), "gone");
}
