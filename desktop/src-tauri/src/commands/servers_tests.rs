use super::*;
use crate::state::test_support::TestApp;

#[test]
fn add_server_rejects_an_empty_or_blank_address() {
    let t = TestApp::new();
    for blank in ["", "   ", "\t\n"] {
        let err = add_server(t.state(), blank.to_string()).err().expect("rejected");
        assert!(matches!(err, AppError::InvalidInput(_)), "{blank:?}");
    }
    assert!(list_servers(t.state()).unwrap().is_empty());
}

#[test]
fn add_server_trims_the_address_and_lists_it() {
    let t = TestApp::new();
    let added = add_server(t.state(), "  hub.example:9000 \n".into()).unwrap();
    assert_eq!(added.address, "hub.example:9000");
    let listed = list_servers(t.state()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, added.id);
}

#[test]
fn remove_server_rejects_ids_that_are_not_uuids() {
    let t = TestApp::new();
    let err = remove_server(t.state(), "not-a-uuid".into()).unwrap_err();
    assert!(err.to_string().contains("not-a-uuid"), "{err}");
}

#[test]
fn remove_server_is_refused_while_its_room_is_active_and_allowed_after() {
    let t = TestApp::new();
    let added = add_server(t.state(), "hub:1".into()).unwrap();
    let id = Uuid::parse_str(&added.id).unwrap();

    *t.state().active_room_server.lock().unwrap() = Some(id);
    assert!(matches!(
        remove_server(t.state(), added.id.clone()),
        Err(AppError::InvalidInput(_))
    ));
    assert_eq!(list_servers(t.state()).unwrap().len(), 1);

    *t.state().active_room_server.lock().unwrap() = None;
    remove_server(t.state(), added.id).unwrap();
    assert!(list_servers(t.state()).unwrap().is_empty());
}

#[test]
fn removing_one_server_leaves_the_others_and_other_active_rooms_do_not_block() {
    let t = TestApp::new();
    let a = add_server(t.state(), "a:1".into()).unwrap();
    let b = add_server(t.state(), "b:2".into()).unwrap();
    *t.state().active_room_server.lock().unwrap() = Some(Uuid::parse_str(&b.id).unwrap());

    remove_server(t.state(), a.id).unwrap();
    let left = list_servers(t.state()).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, b.id);
}

#[test]
fn get_client_id_is_a_stable_uuid() {
    let t = TestApp::new();
    let first = get_client_id(t.state());
    assert!(Uuid::parse_str(&first).is_ok());
    assert_eq!(get_client_id(t.state()), first);
}
