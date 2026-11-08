use super::*;
use crate::state::test_support::TestApp;

#[test]
fn success_maps_to_ok_and_an_agent_error_keeps_its_message() {
    assert!(success_or_err(DaemonResponse::Success {
        request_type: "X".into()
    })
    .is_ok());

    let err = success_or_err(DaemonResponse::Error {
        request_type: "StartRoomSession".into(),
        message: "ticket de sala inválido".into(),
    })
    .unwrap_err();
    assert_eq!(err.to_string(), "ticket de sala inválido");
}

#[test]
fn an_unexpected_agent_reply_is_reported_not_swallowed() {
    let err = success_or_err(DaemonResponse::GameEnded).unwrap_err();
    assert!(err.to_string().contains("respuesta inesperada"), "{err}");
}

#[test]
fn commands_fail_loudly_until_the_agent_is_started() {
    let t = TestApp::new();
    let err = require_agent_state(&t.state()).err().expect("no agent yet");
    assert!(err.to_string().contains("no está iniciado"), "{err}");

    *t.state().agent_state.lock().unwrap() = Some(Arc::new(TokioMutex::new(DaemonState::new())));
    assert!(require_agent_state(&t.state()).is_ok());
}
