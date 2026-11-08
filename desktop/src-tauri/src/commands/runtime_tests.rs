use super::*;
use crate::state::test_support::TestApp;

#[test]
fn success_maps_to_ok_and_a_runtime_error_keeps_its_message() {
    assert!(success_or_err(RuntimeResponse::Success {
        request_type: "X".into()
    })
    .is_ok());

    let err = success_or_err(RuntimeResponse::Error {
        request_type: "StartRoomSession".into(),
        message: "ticket de sala inválido".into(),
    })
    .unwrap_err();
    assert_eq!(err.to_string(), "ticket de sala inválido");
}

#[test]
fn an_unexpected_runtime_reply_is_reported_not_swallowed() {
    let err = success_or_err(RuntimeResponse::GameEnded).unwrap_err();
    assert!(err.to_string().contains("respuesta inesperada"), "{err}");
}

#[test]
fn commands_fail_loudly_until_the_runtime_is_started() {
    let t = TestApp::new();
    let err = require_runtime_state(&t.state()).err().expect("no runtime yet");
    assert!(err.to_string().contains("aún no está iniciada"), "{err}");

    *t.state().runtime_state.lock().unwrap() = Some(Arc::new(TokioMutex::new(RuntimeState::new())));
    assert!(require_runtime_state(&t.state()).is_ok());
}
