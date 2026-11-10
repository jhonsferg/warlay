//! End-to-end tests of the main server's API against real PostgreSQL and
//! Redis. Skipped unless `WARLAY_TEST_DATABASE_URL` and
//! `WARLAY_TEST_REDIS_URL` are set (see tools/dev-stack).

use std::time::Duration;

use base64::Engine;
use futures_util::StreamExt;
use serde_json::{json, Value};
use social::{helpers, MainSettings};
use uuid::Uuid;
use warlay_protocol::{HelperHeartbeat, RoomTicket};

const SECRET: &str = "integration-test-helper-secret";

struct Server {
    base: String,
    /// A second node serving only the edge routes, like a helper.
    edge: String,
    client: reqwest::Client,
    state: social::AppState,
}

async fn start() -> Option<Server> {
    start_with(|_| {}).await
}

async fn start_with(configure: impl FnOnce(&mut MainSettings)) -> Option<Server> {
    let (Ok(database_url), Ok(redis_url)) = (
        std::env::var("WARLAY_TEST_DATABASE_URL"),
        std::env::var("WARLAY_TEST_REDIS_URL"),
    ) else {
        eprintln!("skipping: WARLAY_TEST_DATABASE_URL / WARLAY_TEST_REDIS_URL not set");
        return None;
    };
    let mut settings = MainSettings {
        database_url,
        redis_url,
        helper_secret: SECRET.into(),
        disconnect_grace_secs: 1,
        host_absence_close_secs: 2,
        ..MainSettings::default()
    };
    configure(&mut settings);
    let state = social::connect(settings).await.expect("connect");
    helpers::record_heartbeat(
        &state,
        &HelperHeartbeat {
            helper_id: Uuid::new_v4(),
            name: "test-helper".into(),
            region: "test".into(),
            rendezvous_addr: "127.0.0.1:7779".into(),
            relay_addr: "127.0.0.1:7780".into(),
            active_rooms: 0,
            relay_channels: 0,
            edge_url: String::new(),
            clients: 0,
        },
    )
    .await
    .expect("register helper");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    social::start_chat_persister(&state, &format!("test-{}", Uuid::new_v4()));
    let main_state = state.clone();
    tokio::spawn(async move { axum::serve(listener, social::router(main_state)).await });
    let edge_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let edge_addr = edge_listener.local_addr().unwrap();
    let edge_state = state.clone();
    tokio::spawn(async move { axum::serve(edge_listener, social::edge_router(edge_state)).await });
    Some(Server {
        base: format!("http://{addr}"),
        edge: format!("http://{edge_addr}"),
        client: reqwest::Client::new(),
        state,
    })
}

struct User {
    id: Uuid,
    username: String,
    token: String,
    refresh: String,
}

fn unique(prefix: &str) -> String {
    format!("{prefix}_{}", &Uuid::new_v4().simple().to_string()[..10])
}

impl Server {
    async fn call(&self, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> (u16, Value) {
        let url = format!("{}/api/v1{path}", self.base);
        let mut req = match method {
            "GET" => self.client.get(url),
            "POST" => self.client.post(url),
            "PATCH" => self.client.patch(url),
            "PUT" => self.client.put(url),
            "DELETE" => self.client.delete(url),
            _ => unreachable!(),
        };
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.json(&body);
        }
        let res = req.send().await.expect("request");
        let status = res.status().as_u16();
        let value = res.json::<Value>().await.unwrap_or(Value::Null);
        (status, value)
    }

    async fn ok(&self, method: &str, path: &str, user: &User, body: Option<Value>) -> Value {
        let (status, value) = self.call(method, path, Some(&user.token), body).await;
        assert!((200..300).contains(&status), "{method} {path} -> {status}: {value}");
        value
    }

    async fn register(&self, prefix: &str) -> User {
        let username = unique(prefix);
        let (status, value) = self
            .call(
                "POST",
                "/auth/register",
                None,
                Some(
                    json!({ "username": username, "email": format!("{username}@test.dev"), "password": "supersecret" }),
                ),
            )
            .await;
        assert_eq!(status, 200, "{value}");
        User {
            id: value["user"]["id"].as_str().unwrap().parse().unwrap(),
            username,
            token: value["tokens"]["accessToken"].as_str().unwrap().into(),
            refresh: value["tokens"]["refreshToken"].as_str().unwrap().into(),
        }
    }

    async fn befriend(&self, a: &User, b: &User) {
        self.ok("POST", "/friends/requests", a, Some(json!({ "username": b.username })))
            .await;
        let requests = self.ok("GET", "/friends/requests", b, None).await;
        let id = requests["incoming"][0]["id"].as_str().unwrap().to_string();
        self.ok("POST", &format!("/friends/requests/{id}/accept"), b, None)
            .await;
    }

    async fn socket(
        &self,
        user: &User,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
        self.socket_at(&self.base.clone(), user).await
    }

    async fn socket_at(
        &self,
        base: &str,
        user: &User,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
        let url = format!("{}/api/v1/ws?token={}", base.replace("http", "ws"), user.token);
        let (mut ws, _) = tokio_tungstenite::connect_async(url).await.expect("ws connect");
        let hello = next_event(&mut ws).await;
        assert_eq!(hello["type"], "hello");
        ws
    }
}

async fn next_event<S>(ws: &mut S) -> Value
where
    S: StreamExt<Item = Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for a realtime event")
            .expect("socket closed")
            .expect("socket error");
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            return serde_json::from_str(&text).expect("json event");
        }
    }
}

/// Waits for the first event of type `kind`, skipping others.
async fn wait_for<S>(ws: &mut S, kind: &str) -> Value
where
    S: StreamExt<Item = Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let event = next_event(ws).await;
        if event["type"] == kind {
            return event;
        }
    }
}

#[tokio::test]
async fn accounts_sessions_and_profile() {
    let Some(s) = start().await else { return };
    let user = s.register("acc").await;

    // Duplicate username and bad credentials.
    let (status, value) = s
        .call(
            "POST",
            "/auth/register",
            None,
            Some(json!({ "username": user.username, "email": "other@test.dev", "password": "supersecret" })),
        )
        .await;
    assert_eq!((status, value["error"].as_str()), (409, Some("username_taken")));
    let (status, _) = s
        .call(
            "POST",
            "/auth/login",
            None,
            Some(json!({ "login": user.username, "password": "nope-nope" })),
        )
        .await;
    assert_eq!(status, 401);

    // Login by email, profile update.
    let (status, login) = s
        .call(
            "POST",
            "/auth/login",
            None,
            Some(json!({ "login": format!("{}@TEST.dev", user.username), "password": "supersecret" })),
        )
        .await;
    assert_eq!(status, 200, "{login}");
    let me = s
        .ok(
            "PATCH",
            "/me",
            &user,
            Some(json!({ "displayName": "Thrall", "bio": "Por la Horda" })),
        )
        .await;
    assert_eq!(me["displayName"], "Thrall");

    // Refresh rotates the refresh token.
    let (status, tokens) = s
        .call(
            "POST",
            "/auth/refresh",
            None,
            Some(json!({ "refreshToken": user.refresh })),
        )
        .await;
    assert_eq!(status, 200);
    let (status, _) = s
        .call(
            "POST",
            "/auth/refresh",
            None,
            Some(json!({ "refreshToken": user.refresh })),
        )
        .await;
    assert_eq!(status, 401, "an old refresh token must not work twice");

    // Logout kills the access token.
    let fresh = User {
        token: tokens["accessToken"].as_str().unwrap().into(),
        ..user
    };
    s.ok("POST", "/auth/logout", &fresh, None).await;
    let (status, _) = s.call("GET", "/me", Some(&fresh.token), None).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn friends_presence_and_direct_messages() {
    let Some(s) = start().await else { return };
    let a = s.register("fa").await;
    let b = s.register("fb").await;
    let mut ws_b = s.socket(&b).await;

    s.ok("POST", "/friends/requests", &a, Some(json!({ "username": b.username })))
        .await;
    let event = wait_for(&mut ws_b, "friendRequest").await;
    assert_eq!(event["request"]["from"]["id"], a.id.to_string());
    let notification = wait_for(&mut ws_b, "notification").await;
    assert_eq!(notification["notification"]["kind"], "friend_request");

    let requests = s.ok("GET", "/friends/requests", &b, None).await;
    let id = requests["incoming"][0]["id"].as_str().unwrap().to_string();
    s.ok("POST", &format!("/friends/requests/{id}/accept"), &b, None).await;
    let added = wait_for(&mut ws_b, "friendAdded").await;
    assert_eq!(added["friend"]["id"], a.id.to_string());

    // B is connected, so A sees them online.
    let friends = s.ok("GET", "/friends", &a, None).await;
    assert_eq!(friends[0]["id"], b.id.to_string());
    assert_eq!(friends[0]["presence"]["status"], "online");

    // Direct messages reach the other side live and are stored.
    s.ok(
        "POST",
        &format!("/chat/dm/{}/messages", b.id),
        &a,
        Some(json!({ "body": "gg wp" })),
    )
    .await;
    let message = wait_for(&mut ws_b, "message").await;
    assert_eq!(message["message"]["body"], "gg wp");
    let history = s.ok("GET", &format!("/chat/dm/{}/messages", a.id), &b, None).await;
    assert_eq!(history.as_array().unwrap().len(), 1);

    // Strangers can't DM.
    let c = s.register("fc").await;
    let (status, _) = s
        .call(
            "POST",
            &format!("/chat/dm/{}/messages", a.id),
            Some(&c.token),
            Some(json!({ "body": "hi" })),
        )
        .await;
    assert_eq!(status, 403);

    // Removing the friendship notifies both.
    s.ok("DELETE", &format!("/friends/{}", b.id), &a, None).await;
    let removed = wait_for(&mut ws_b, "friendRemoved").await;
    assert_eq!(removed["userId"], a.id.to_string());
}

#[tokio::test]
async fn communities_invites_and_chat() {
    let Some(s) = start().await else { return };
    let owner = s.register("co").await;
    let guest = s.register("cg").await;
    let name = unique("Clan");

    let private = s
        .ok(
            "POST",
            "/communities",
            &owner,
            Some(json!({ "name": name, "isPublic": false })),
        )
        .await;
    let id = private["id"].as_str().unwrap().to_string();
    assert_eq!(private["myRole"], "owner");

    // Private: no self-join, invite only.
    let (status, _) = s
        .call("POST", &format!("/communities/{id}/join"), Some(&guest.token), None)
        .await;
    assert_eq!(status, 403);
    let mut ws_guest = s.socket(&guest).await;
    s.ok(
        "POST",
        &format!("/communities/{id}/invites"),
        &owner,
        Some(json!({ "userId": guest.id })),
    )
    .await;
    let invite = wait_for(&mut ws_guest, "invite").await;
    let invite_id = invite["invite"]["id"].as_str().unwrap().to_string();
    s.ok("POST", &format!("/invites/{invite_id}/accept"), &guest, None)
        .await;

    let detail = s.ok("GET", &format!("/communities/{id}"), &guest, None).await;
    assert_eq!(detail["memberCount"], 2);
    assert_eq!(detail["members"][0]["role"], "owner");

    s.ok(
        "POST",
        &format!("/chat/community/{id}/messages"),
        &owner,
        Some(json!({ "body": "bienvenido" })),
    )
    .await;
    assert_eq!(
        wait_for(&mut ws_guest, "message").await["message"]["body"],
        "bienvenido"
    );

    // Public communities show up in discover and can be joined freely.
    let public_name = unique("Publica");
    let public = s
        .ok("POST", "/communities", &owner, Some(json!({ "name": public_name })))
        .await;
    let found = s
        .ok("GET", &format!("/communities/discover?q={public_name}"), &guest, None)
        .await;
    assert_eq!(found[0]["id"], public["id"]);
    s.ok(
        "POST",
        &format!("/communities/{}/join", public["id"].as_str().unwrap()),
        &guest,
        None,
    )
    .await;
    let mine = s.ok("GET", "/communities", &guest, None).await;
    assert_eq!(mine.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn rooms_sessions_tickets_and_lifecycle() {
    let Some(s) = start().await else { return };
    let host = s.register("rh").await;
    let friend = s.register("rf").await;
    let other = s.register("ro").await;
    s.befriend(&host, &friend).await;

    let session = s
        .ok(
            "POST",
            "/rooms",
            &host,
            Some(json!({ "name": "DotA 6.83", "password": "orcs", "maxPlayers": 3 })),
        )
        .await;
    let room_id = session["room"]["id"].as_str().unwrap().to_string();
    assert_eq!(session["isHost"], true);
    assert_eq!(session["helper"]["rendezvousAddr"], "127.0.0.1:7779");

    // The ticket is signed with the helper secret and names this room/user.
    let raw = base64::engine::general_purpose::STANDARD
        .decode(session["ticket"].as_str().unwrap())
        .unwrap();
    let payload = warlay_crypto::verify_token(SECRET.as_bytes(), &raw).expect("valid signature");
    let ticket = RoomTicket::decode(payload).unwrap();
    assert_eq!(ticket.room_id.to_string(), room_id);
    assert_eq!(ticket.user_id, host.id);
    assert!(ticket.is_host);

    // Wrong password, then right one.
    let (status, value) = s
        .call(
            "POST",
            &format!("/rooms/{room_id}/join"),
            Some(&other.token),
            Some(json!({ "password": "x" })),
        )
        .await;
    assert_eq!((status, value["error"].as_str()), (403, Some("wrong_password")));
    let joined = s
        .ok(
            "POST",
            &format!("/rooms/{room_id}/join"),
            &other,
            Some(json!({ "password": "orcs" })),
        )
        .await;
    assert_eq!(joined["isHost"], false);
    assert_eq!(joined["room"]["members"].as_array().unwrap().len(), 2);

    // Invite a friend: the invite skips the password.
    let mut ws_friend = s.socket(&friend).await;
    s.ok(
        "POST",
        &format!("/rooms/{room_id}/invites"),
        &host,
        Some(json!({ "userId": friend.id })),
    )
    .await;
    let invite = wait_for(&mut ws_friend, "invite").await;
    let accepted = s
        .ok(
            "POST",
            &format!("/invites/{}/accept", invite["invite"]["id"].as_str().unwrap()),
            &friend,
            None,
        )
        .await;
    assert_eq!(accepted["session"]["room"]["id"], room_id.as_str());

    // Full now (3/3).
    let late = s.register("rl").await;
    let (status, value) = s
        .call(
            "POST",
            &format!("/rooms/{room_id}/join"),
            Some(&late.token),
            Some(json!({ "password": "orcs" })),
        )
        .await;
    assert_eq!((status, value["error"].as_str()), (409, Some("room_full")));

    // Room chat and status; only the host may change it.
    s.ok(
        "POST",
        &format!("/chat/room/{room_id}/messages"),
        &other,
        Some(json!({ "body": "listo" })),
    )
    .await;
    assert_eq!(wait_for(&mut ws_friend, "message").await["message"]["body"], "listo");
    let (status, _) = s
        .call(
            "POST",
            &format!("/rooms/{room_id}/status"),
            Some(&other.token),
            Some(json!({ "status": "in_game" })),
        )
        .await;
    assert_eq!(status, 403);
    s.ok(
        "POST",
        &format!("/rooms/{room_id}/status"),
        &host,
        Some(json!({ "status": "in_game" })),
    )
    .await;
    let friends = s.ok("GET", "/friends", &host, None).await;
    assert_eq!(friends[0]["presence"]["status"], "in_game");

    // A running game doesn't lock the room: a newcomer can still join.
    s.ok("POST", &format!("/rooms/{room_id}/leave"), &other, None).await;
    let rejoined = s
        .ok(
            "POST",
            &format!("/rooms/{room_id}/join"),
            &late,
            Some(json!({ "password": "orcs" })),
        )
        .await;
    assert_eq!(rejoined["room"]["status"], "in_game");

    // The host leaving closes the room for everyone.
    s.ok("POST", &format!("/rooms/{room_id}/leave"), &host, None).await;
    wait_for_room(&mut ws_friend, "roomClosed", &room_id).await;
    let current = s.ok("GET", "/rooms/current", &friend, None).await;
    assert!(current.is_null());
}

#[tokio::test]
async fn disconnect_grace_removes_player_from_room() {
    let Some(s) = start().await else { return };
    let host = s.register("dh").await;
    let player = s.register("dp").await;
    let session = s.ok("POST", "/rooms", &host, Some(json!({ "name": "Grace" }))).await;
    let room_id = session["room"]["id"].as_str().unwrap().to_string();
    s.ok("POST", &format!("/rooms/{room_id}/join"), &player, None).await;

    let ws = s.socket(&player).await;
    drop(ws);
    // disconnect_grace_secs = 1 in these tests.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let room = s.ok("GET", &format!("/rooms/{room_id}"), &host, None).await;
    assert_eq!(room["members"].as_array().unwrap().len(), 1);
}

/// Chat through a helper's edge: sent over the edge socket, delivered to a
/// player connected to the *other* node (via the Redis bus), served back
/// from the Redis recent list, and persisted to PostgreSQL in the
/// background -- the main server's own routes are never involved.
#[tokio::test]
async fn chat_flows_through_edges_and_is_persisted() {
    use futures_util::SinkExt;
    let Some(s) = start().await else { return };
    let a = s.register("ea").await;
    let b = s.register("eb").await;
    s.befriend(&a, &b).await;

    let edge = s.edge.clone();
    let mut ws_a = s.socket_at(&edge, &a).await;
    let mut ws_b = s.socket(&b).await; // B sits on the other node.

    let request = json!({
        "type": "chat.send", "requestId": "r1", "kind": "dm", "targetId": b.id, "body": "vía el ayudante"
    });
    ws_a.send(tokio_tungstenite::tungstenite::Message::Text(request.to_string()))
        .await
        .unwrap();
    let ack = wait_for(&mut ws_a, "chat.ack").await;
    assert_eq!(ack["requestId"], "r1");
    let id = ack["message"]["id"].as_str().unwrap().to_string();
    assert_eq!(wait_for(&mut ws_b, "message").await["message"]["id"], id.as_str());

    // Not allowed for strangers, answered on the same socket.
    let stranger = s.register("ec").await;
    let bad = json!({ "type": "chat.send", "requestId": "r2", "kind": "dm", "targetId": stranger.id, "body": "hola" });
    ws_a.send(tokio_tungstenite::tungstenite::Message::Text(bad.to_string()))
        .await
        .unwrap();
    let error = wait_for(&mut ws_a, "chat.error").await;
    assert_eq!(
        (error["requestId"].as_str(), error["error"].as_str()),
        (Some("r2"), Some("forbidden"))
    );

    // History from the edge's REST route.
    let history: Value = s
        .client
        .get(format!("{edge}/api/v1/chat/dm/{}/messages", a.id))
        .bearer_auth(&b.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(history[0]["id"], id.as_str());

    // The persister lands it in PostgreSQL shortly after.
    let message_id: Uuid = id.parse().unwrap();
    let mut stored = 0_i64;
    for _ in 0..50 {
        stored = sqlx::query_scalar("SELECT count(*) FROM messages WHERE id = $1")
            .bind(message_id)
            .fetch_one(&s.state.db)
            .await
            .unwrap();
        if stored == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(stored, 1, "chat message was never persisted");

    // The edge list advertises helpers that have an edge.
    helpers::record_heartbeat(
        &s.state,
        &HelperHeartbeat {
            helper_id: Uuid::new_v4(),
            name: "edge-test".into(),
            region: "test".into(),
            rendezvous_addr: "127.0.0.1:7779".into(),
            relay_addr: "127.0.0.1:7780".into(),
            active_rooms: 0,
            relay_channels: 0,
            edge_url: edge.clone(),
            clients: 0,
        },
    )
    .await
    .unwrap();
    let edges = s.ok("GET", "/edges", &a, None).await;
    assert!(edges.as_array().unwrap().iter().any(|e| e["url"] == edge.as_str()));
}

/// Polls `path` until `done` accepts its answer (the chat persister writes
/// to PostgreSQL a moment after a message is sent).
async fn eventually(s: &Server, user: &User, path: &str, done: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..50 {
        let value = s.ok("GET", path, user, None).await;
        if done(&value) {
            return value;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("{path} never reached the expected state");
}

#[tokio::test]
async fn dm_receipts_unread_counts_and_deletion() {
    let Some(s) = start().await else { return };
    let a = s.register("ra").await;
    let b = s.register("rb").await;
    s.befriend(&a, &b).await;
    let mut ws_a = s.socket(&a).await;
    let mut ws_b = s.socket(&b).await;

    let to_b = format!("/chat/dm/{}/messages", b.id);
    let first = s.ok("POST", &to_b, &a, Some(json!({ "body": "uno" }))).await;
    let second = s.ok("POST", &to_b, &a, Some(json!({ "body": "dos" }))).await;
    wait_for(&mut ws_b, "message").await;
    wait_for(&mut ws_b, "message").await;

    // B hasn't read anything: two unread, and A's messages are still "sent".
    let unread = eventually(&s, &b, "/chat/unread", |v| v[0]["count"] == 2).await;
    assert_eq!(unread[0]["peerId"], a.id.to_string());
    let peer = s.ok("GET", &format!("/chat/dm/{}/receipt", b.id), &a, None).await;
    let created = |m: &Value| m["createdAt"].as_str().unwrap().to_string();
    assert!(peer["deliveredAt"].as_str().unwrap() < created(&first).as_str());

    // B's device acknowledges delivery of both: A sees it live.
    s.ok(
        "POST",
        &format!("/chat/dm/{}/receipt", a.id),
        &b,
        Some(json!({ "state": "delivered", "upTo": created(&second) })),
    )
    .await;
    let seen = wait_for(&mut ws_a, "chatReceipt").await;
    assert_eq!(seen["userId"], b.id.to_string());
    assert_eq!(seen["receipt"]["deliveredAt"], second["createdAt"]);
    // Delivered is not read: the unread count stays.
    assert_eq!(s.ok("GET", "/chat/unread", &b, None).await[0]["count"], 2);

    // Reading the first message only leaves one unread.
    s.ok(
        "POST",
        &format!("/chat/dm/{}/receipt", a.id),
        &b,
        Some(json!({ "state": "read", "upTo": created(&first) })),
    )
    .await;
    wait_for(&mut ws_a, "chatReceipt").await;
    assert_eq!(s.ok("GET", "/chat/unread", &b, None).await[0]["count"], 1);

    // Cursors never move back.
    s.ok(
        "POST",
        &format!("/chat/dm/{}/receipt", a.id),
        &b,
        Some(json!({ "state": "read", "upTo": "2000-01-01T00:00:00Z" })),
    )
    .await;
    let peer = s.ok("GET", &format!("/chat/dm/{}/receipt", b.id), &a, None).await;
    assert_eq!(peer["readAt"], first["createdAt"]);

    // Only the sender deletes, and everyone in the channel hears about it.
    let second_id = second["id"].as_str().unwrap();
    let (status, _) = s
        .call(
            "DELETE",
            &format!("/chat/dm/{}/messages/{second_id}", a.id),
            Some(&b.token),
            None,
        )
        .await;
    assert_eq!(status, 403);
    s.ok("DELETE", &format!("/chat/dm/{}/messages/{second_id}", b.id), &a, None)
        .await;
    let deleted = wait_for(&mut ws_b, "messageDeleted").await;
    assert_eq!(deleted["messageId"], second_id);

    // Gone from history, from the recent cache and from the unread count --
    // even though the persister may only have written it after the delete.
    let history = eventually(&s, &b, &format!("/chat/dm/{}/messages", a.id), |v| {
        v.as_array().unwrap().len() == 1
    })
    .await;
    assert_eq!(history[0]["body"], "uno");
    let unread = eventually(&s, &b, "/chat/unread", |v| v.as_array().unwrap().is_empty()).await;
    assert!(unread.as_array().unwrap().is_empty());

    // Receipts only exist for direct messages.
    let (status, _) = s
        .call(
            "POST",
            &format!("/chat/community/{}/receipt", a.id),
            Some(&a.token),
            Some(json!({ "state": "read", "upTo": "2030-01-01T00:00:00Z" })),
        )
        .await;
    assert_eq!(status, 400);
}

/// The next event of type `kind` about room `room_id`: public rooms are
/// announced to every connected user, so other tests' rooms show up too.
async fn wait_for_room<S>(ws: &mut S, kind: &str, room_id: &str) -> Value
where
    S: StreamExt<Item = Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let event = wait_for(ws, kind).await;
        if event["room"]["id"] == room_id || event["roomId"] == room_id {
            return event;
        }
    }
}

/// Waits until room `room_id` is announced with `hostAway == away`; earlier
/// updates (coalesced joins, say) are skipped.
async fn wait_for_host_away<S>(ws: &mut S, room_id: &str, away: bool)
where
    S: StreamExt<Item = Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        if wait_for_room(ws, "roomUpdated", room_id).await["room"]["hostAway"] == away {
            return;
        }
    }
}

#[tokio::test]
async fn a_room_outlives_its_host_disconnecting() {
    let Some(s) = start().await else { return };
    let host = s.register("ph").await;
    let player = s.register("pp").await;
    let latecomer = s.register("pl").await;
    let session = s
        .ok("POST", "/rooms", &host, Some(json!({ "name": "Sobrevive" })))
        .await;
    let room_id = session["room"]["id"].as_str().unwrap().to_string();
    s.ok("POST", &format!("/rooms/{room_id}/join"), &player, None).await;

    let mut ws_player = s.socket(&player).await;
    let ws_host = s.socket(&host).await;
    drop(ws_host);

    // Past the grace period (1s) the host is flagged away, not removed.
    wait_for_host_away(&mut ws_player, &room_id, true).await;
    let room = s.ok("GET", &format!("/rooms/{room_id}"), &player, None).await;
    assert_eq!(room["members"].as_array().unwrap().len(), 2);
    assert_eq!(room["status"], "open");

    // The room keeps taking players while its host is gone...
    s.ok("POST", &format!("/rooms/{room_id}/join"), &latecomer, None).await;
    // ...and the host can't wander off into another one and take it down.
    let (status, value) = s
        .call("POST", "/rooms", Some(&host.token), Some(json!({ "name": "Otra" })))
        .await;
    assert_eq!((status, value["error"].as_str()), (409, Some("hosting")));

    // Back online, they get the room back and can manage it as before.
    let _ws_host = s.socket(&host).await;
    wait_for_host_away(&mut ws_player, &room_id, false).await;
    let current = s.ok("GET", "/rooms/current", &host, None).await;
    assert_eq!(current["isHost"], true);
    assert_eq!(current["room"]["id"], room_id.as_str());
    s.ok(
        "POST",
        &format!("/rooms/{room_id}/status"),
        &host,
        Some(json!({ "status": "in_game" })),
    )
    .await;

    // Only the host closing it ends the room.
    s.ok("DELETE", &format!("/rooms/{room_id}"), &host, None).await;
    wait_for_room(&mut ws_player, "roomClosed", &room_id).await;
}

#[tokio::test]
async fn rooms_whose_host_never_returns_are_eventually_closed() {
    let Some(s) = start().await else { return };
    let host = s.register("ah").await;
    let player = s.register("ap").await;
    let session = s
        .ok("POST", "/rooms", &host, Some(json!({ "name": "Abandonada" })))
        .await;
    let room_id = session["room"]["id"].as_str().unwrap().to_string();
    s.ok("POST", &format!("/rooms/{room_id}/join"), &player, None).await;

    let mut ws_player = s.socket(&player).await;
    drop(s.socket(&host).await);
    wait_for_host_away(&mut ws_player, &room_id, true).await;

    // Not yet: the host has only just left (host_absence_close_secs = 2).
    social::reap_abandoned_rooms(&s.state).await;
    assert!(!s.ok("GET", "/rooms/current", &player, None).await.is_null());

    tokio::time::sleep(Duration::from_millis(2500)).await;
    social::reap_abandoned_rooms(&s.state).await;
    wait_for_room(&mut ws_player, "roomClosed", &room_id).await;
    assert!(s.ok("GET", "/rooms/current", &player, None).await.is_null());
}

#[tokio::test]
async fn e2e_keys_and_encrypted_direct_messages() {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use warlay_crypto::e2e::{decrypt_dm, encrypt_dm, Identity};

    let Some(s) = start().await else { return };
    let a = s.register("ka").await;
    let b = s.register("kb").await;
    let stranger = s.register("ks").await;
    s.befriend(&a, &b).await;
    let (ida, idb) = (Identity::generate(), Identity::generate());
    let b64 = |key: [u8; 32]| URL_SAFE_NO_PAD.encode(key);

    // No key yet; garbage and the all-zero point are refused.
    let (status, _) = s
        .call("GET", &format!("/users/{}/e2e-key", b.id), Some(&a.token), None)
        .await;
    assert_eq!(status, 404);
    for bad in ["", "AAAA", &b64([0; 32])] {
        let (status, value) = s
            .call("PUT", "/me/e2e-key", Some(&a.token), Some(json!({ "publicKey": bad })))
            .await;
        assert_eq!((status, value["error"].as_str()), (400, Some("invalid_key")));
    }

    // Publish; republishing the same key isn't a change, a new one is.
    let first = s
        .ok(
            "PUT",
            "/me/e2e-key",
            &a,
            Some(json!({ "publicKey": b64(ida.public_bytes()) })),
        )
        .await;
    assert_eq!(first["changed"], false);
    let again = s
        .ok(
            "PUT",
            "/me/e2e-key",
            &a,
            Some(json!({ "publicKey": b64(ida.public_bytes()) })),
        )
        .await;
    assert_eq!(again["changed"], false);
    assert_eq!(again["updatedAt"], first["updatedAt"]);
    s.ok(
        "PUT",
        "/me/e2e-key",
        &b,
        Some(json!({ "publicKey": b64(idb.public_bytes()) })),
    )
    .await;

    // Friends (and oneself) see it; strangers don't.
    let seen = s.ok("GET", &format!("/users/{}/e2e-key", a.id), &b, None).await;
    assert_eq!(seen["publicKey"], b64(ida.public_bytes()));
    s.ok("GET", &format!("/users/{}/e2e-key", a.id), &a, None).await;
    let (status, _) = s
        .call("GET", &format!("/users/{}/e2e-key", a.id), Some(&stranger.token), None)
        .await;
    assert_eq!(status, 403);

    // An encrypted DM, far longer than a plain one may be, goes through
    // untouched, and only its two parties can read it.
    let text = "ñ".repeat(1900);
    let envelope = encrypt_dm(
        &ida,
        &idb.public_bytes(),
        *a.id.as_bytes(),
        *b.id.as_bytes(),
        "req-e2e",
        text.as_bytes(),
    )
    .unwrap();
    assert!(envelope.len() > 2000);
    let sent = s
        .ok(
            "POST",
            &format!("/chat/dm/{}/messages", b.id),
            &a,
            Some(json!({ "body": envelope })),
        )
        .await;
    assert_eq!(sent["body"], envelope.as_str());
    let history = s.ok("GET", &format!("/chat/dm/{}/messages", a.id), &b, None).await;
    let stored = history[0]["body"].as_str().unwrap();
    assert!(!stored.contains('ñ'));
    let opened = decrypt_dm(
        &idb,
        &ida.public_bytes(),
        *a.id.as_bytes(),
        *b.id.as_bytes(),
        stored,
        false,
    )
    .unwrap();
    assert_eq!(opened.plaintext, text.as_bytes());
    let mine = decrypt_dm(
        &ida,
        &idb.public_bytes(),
        *a.id.as_bytes(),
        *b.id.as_bytes(),
        stored,
        true,
    )
    .unwrap();
    assert_eq!(mine.plaintext, text.as_bytes());

    // The larger limit is only for real envelopes, in DMs.
    let padded = format!("e2e1:{}", "A".repeat(5000));
    s.ok(
        "POST",
        &format!("/chat/dm/{}/messages", b.id),
        &a,
        Some(json!({ "body": padded })),
    )
    .await;
    for body in [format!("e2e1: {}", "A".repeat(5000)), "x".repeat(2001)] {
        let (status, value) = s
            .call(
                "POST",
                &format!("/chat/dm/{}/messages", b.id),
                Some(&a.token),
                Some(json!({ "body": body })),
            )
            .await;
        assert_eq!((status, value["error"].as_str()), (400, Some("message_too_long")));
    }
}

#[tokio::test]
async fn offline_friends_report_when_they_were_last_seen() {
    let Some(s) = start().await else { return };
    let a = s.register("ls_a").await;
    let b = s.register("ls_b").await;
    let c = s.register("ls_c").await;
    s.befriend(&a, &b).await;
    s.befriend(&a, &c).await;

    // B connects and leaves; C never connects at all.
    let ws_b = s.socket(&b).await;
    let friends = s.ok("GET", "/friends", &a, None).await;
    let find = |friends: &Value, id: Uuid| {
        friends
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == id.to_string())
            .cloned()
            .expect("friend present")
    };
    // Online: no "last seen" (they are here right now).
    let online = find(&friends, b.id);
    assert_eq!(online["presence"]["status"], "online");
    assert!(online["presence"]["lastSeenAt"].is_null());

    drop(ws_b);
    let friends = eventually(&s, &a, "/friends", |v| {
        v.as_array()
            .unwrap()
            .iter()
            .any(|f| f["id"] == b.id.to_string() && f["presence"]["status"] == "offline")
    })
    .await;
    let offline = find(&friends, b.id);
    let seen = offline["presence"]["lastSeenAt"]
        .as_str()
        .expect("lastSeenAt for an offline friend");
    let seen = chrono::DateTime::parse_from_rfc3339(seen).expect("RFC 3339 timestamp");
    let age = chrono::Utc::now().signed_duration_since(seen).num_seconds();
    assert!((0..60).contains(&age), "seen {age}s ago");

    // Never connected: offline, and honestly no timestamp.
    let never = find(&friends, c.id);
    assert_eq!(never["presence"]["status"], "offline");
    assert!(never["presence"]["lastSeenAt"].is_null());

    // The profile carries it too.
    let profile = s.ok("GET", &format!("/users/{}", b.id), &a, None).await;
    assert!(profile["presence"]["lastSeenAt"].is_string());
}

#[tokio::test]
async fn notifications_can_be_cleared_one_by_one_or_all_at_once() {
    let Some(s) = start().await else { return };
    let a = s.register("nt_a").await;
    let b = s.register("nt_b").await;
    let c = s.register("nt_c").await;

    // B receives two friend requests; A keeps one of its own.
    s.ok("POST", "/friends/requests", &a, Some(json!({ "username": b.username })))
        .await;
    s.ok("POST", "/friends/requests", &c, Some(json!({ "username": b.username })))
        .await;
    let requests = s.ok("GET", "/friends/requests", &b, None).await;
    let from_a = requests["incoming"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["from"]["id"] == a.id.to_string())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    s.ok("POST", &format!("/friends/requests/{from_a}/accept"), &b, None)
        .await;

    let before_b = s.ok("GET", "/notifications", &b, None).await;
    assert_eq!(before_b.as_array().unwrap().len(), 2);
    let before_a = s.ok("GET", "/notifications", &a, None).await;
    assert_eq!(before_a.as_array().unwrap().len(), 1, "A: friend_accepted");

    // One by one.
    let first = before_b[0]["id"].as_str().unwrap().to_string();
    s.ok("DELETE", &format!("/notifications/{first}"), &b, None).await;
    let after_one = s.ok("GET", "/notifications", &b, None).await;
    assert_eq!(after_one.as_array().unwrap().len(), 1);
    assert_ne!(after_one[0]["id"], first.as_str());

    // Nobody can delete somebody else's notification.
    let others = before_a[0]["id"].as_str().unwrap().to_string();
    s.ok("DELETE", &format!("/notifications/{others}"), &b, None).await;
    assert_eq!(
        s.ok("GET", "/notifications", &a, None).await.as_array().unwrap().len(),
        1
    );

    // All at once, only mine.
    s.ok("DELETE", "/notifications", &b, None).await;
    assert!(s
        .ok("GET", "/notifications", &b, None)
        .await
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        s.ok("GET", "/notifications", &a, None).await.as_array().unwrap().len(),
        1
    );

    // Clearing the bell never discards what still needs an answer: C's
    // friend request to B is still pending.
    let pending = s.ok("GET", "/friends/requests", &b, None).await;
    assert_eq!(pending["incoming"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn deleting_a_community_removes_it_and_its_pending_invites_for_everyone() {
    let Some(s) = start().await else { return };
    let owner = s.register("dc_o").await;
    let member = s.register("dc_m").await;
    let invited = s.register("dc_i").await;
    s.befriend(&owner, &member).await;
    s.befriend(&owner, &invited).await;

    let community = s
        .ok(
            "POST",
            "/communities",
            &owner,
            Some(json!({ "name": unique("Borrable"), "isPublic": false })),
        )
        .await;
    let id = community["id"].as_str().unwrap().to_string();
    s.ok(
        "POST",
        &format!("/communities/{id}/invites"),
        &owner,
        Some(json!({ "userId": member.id })),
    )
    .await;
    let invite = s.ok("GET", "/invites", &member, None).await;
    let invite_id = invite[0]["id"].as_str().unwrap().to_string();
    s.ok("POST", &format!("/invites/{invite_id}/accept"), &member, None)
        .await;
    // A second invite stays unanswered.
    s.ok(
        "POST",
        &format!("/communities/{id}/invites"),
        &owner,
        Some(json!({ "userId": invited.id })),
    )
    .await;
    assert_eq!(
        s.ok("GET", "/invites", &invited, None).await.as_array().unwrap().len(),
        1
    );

    // Only the owner may delete it.
    let (status, _) = s
        .call("DELETE", &format!("/communities/{id}"), Some(&member.token), None)
        .await;
    assert_eq!(status, 403);
    assert_eq!(
        s.ok("GET", &format!("/communities/{id}"), &member, None).await["memberCount"],
        2
    );

    let mut ws_member = s.socket(&member).await;
    s.ok("DELETE", &format!("/communities/{id}"), &owner, None).await;

    // Members are told, and it is gone for everybody -- including the invite.
    let event = wait_for(&mut ws_member, "communityUpdated").await;
    assert_eq!(event["communityId"], id.as_str());
    for user in [&owner, &member] {
        let (status, _) = s
            .call("GET", &format!("/communities/{id}"), Some(&user.token), None)
            .await;
        assert_eq!(status, 404, "{} still sees the community", user.username);
        assert!(s
            .ok("GET", "/communities", user, None)
            .await
            .as_array()
            .unwrap()
            .is_empty());
    }
    assert!(
        s.ok("GET", "/invites", &invited, None)
            .await
            .as_array()
            .unwrap()
            .is_empty(),
        "the pending invite to a deleted community must disappear"
    );
}

#[tokio::test]
async fn recovery_codes_reset_a_password_and_close_every_session() {
    let Some(s) = start().await else { return };
    let username = unique("rec");
    let (status, signup) = s
        .call(
            "POST",
            "/auth/register",
            None,
            Some(json!({ "username": username, "email": format!("{username}@test.dev"), "password": "supersecret" })),
        )
        .await;
    assert_eq!(status, 200, "{signup}");
    let codes: Vec<String> = signup["recoveryCodes"]
        .as_array()
        .expect("sign-up returns the codes")
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect();
    assert_eq!(codes.len(), 8);
    let user = User {
        id: signup["user"]["id"].as_str().unwrap().parse().unwrap(),
        username: username.clone(),
        token: signup["tokens"]["accessToken"].as_str().unwrap().into(),
        refresh: signup["tokens"]["refreshToken"].as_str().unwrap().into(),
    };
    assert_eq!(s.ok("GET", "/me/recovery-codes", &user, None).await["remaining"], 8);

    let recover = |code: &str, password: &str| {
        s.call(
            "POST",
            "/auth/recover",
            None,
            Some(json!({ "login": username.to_uppercase(), "code": code, "newPassword": password })),
        )
    };

    // A weak password is refused without spending the code.
    let (status, value) = recover(&codes[0], "short").await;
    assert_eq!((status, value["error"].as_str()), (400, Some("weak_password")));
    // A wrong code and a wrong account look the same.
    let (status, value) = recover("ZZZZZ-ZZZZZ", "new-password-1").await;
    assert_eq!((status, value["error"].as_str()), (400, Some("invalid_recovery")));
    assert_eq!(s.ok("GET", "/me/recovery-codes", &user, None).await["remaining"], 8);

    // The code works (typed in lower case and without the dash), once.
    let typed = codes[0].replace('-', "").to_lowercase();
    let (status, value) = recover(&typed, "new-password-1").await;
    assert_eq!((status, value["remaining"].as_i64()), (200, Some(7)), "{value}");
    let (status, value) = recover(&codes[0], "new-password-2").await;
    assert_eq!(
        (status, value["error"].as_str()),
        (400, Some("invalid_recovery")),
        "a code is single use"
    );

    // Every session is closed: the old access token, the old refresh token.
    let (status, _) = s.call("GET", "/me", Some(&user.token), None).await;
    assert_eq!(status, 401, "the old access token must stop working at once");
    let (status, _) = s
        .call(
            "POST",
            "/auth/refresh",
            None,
            Some(json!({ "refreshToken": user.refresh })),
        )
        .await;
    assert_eq!(status, 401);
    // The old password is gone and the new one works.
    let login = |password: &str| {
        s.call(
            "POST",
            "/auth/login",
            None,
            Some(json!({ "login": username, "password": password })),
        )
    };
    assert_eq!(login("supersecret").await.0, 401);
    let (status, session) = login("new-password-1").await;
    assert_eq!(status, 200, "{session}");
    let user = User {
        token: session["tokens"]["accessToken"].as_str().unwrap().into(),
        refresh: session["tokens"]["refreshToken"].as_str().unwrap().into(),
        ..user
    };

    // New codes need the password and replace the old set.
    let (status, value) = s
        .call(
            "POST",
            "/me/recovery-codes",
            Some(&user.token),
            Some(json!({ "password": "wrong-password" })),
        )
        .await;
    assert_eq!((status, value["error"].as_str()), (400, Some("invalid_credentials")));
    let fresh = s
        .ok(
            "POST",
            "/me/recovery-codes",
            &user,
            Some(json!({ "password": "new-password-1" })),
        )
        .await;
    let fresh: Vec<String> = fresh["codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().into())
        .collect();
    assert_eq!(fresh.len(), 8);
    assert_eq!(s.ok("GET", "/me/recovery-codes", &user, None).await["remaining"], 8);
    let (status, value) = recover(&codes[1], "new-password-3").await;
    assert_eq!(
        (status, value["error"].as_str()),
        (400, Some("invalid_recovery")),
        "the old set is gone"
    );
    let (status, _) = recover(&fresh[0], "new-password-3").await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn the_website_serves_pages_in_both_languages_with_safe_headers() {
    let Some(s) = start().await else { return };
    let get = |path: &str, accept: Option<&str>| {
        let mut request = s.client.get(format!("{}{path}", s.base));
        if let Some(accept) = accept {
            request = request.header("accept-language", accept);
        }
        request.send()
    };

    // Spanish is the default, and the browser's language is honoured.
    let es = get("/", None).await.unwrap();
    assert_eq!(es.status(), 200);
    assert_eq!(es.headers()["content-language"], "es");
    assert!(es.headers()["content-type"].to_str().unwrap().starts_with("text/html"));
    let csp = es.headers()["content-security-policy"].to_str().unwrap().to_string();
    assert!(
        csp.contains("default-src 'none'") && csp.contains("frame-ancestors 'none'"),
        "{csp}"
    );
    assert!(es
        .text()
        .await
        .unwrap()
        .contains("Juega Warcraft III en LAN por internet"));

    let en = get("/", Some("en-US,en;q=0.9")).await.unwrap();
    assert_eq!(en.headers()["content-language"], "en");
    assert!(en
        .text()
        .await
        .unwrap()
        .contains("Play Warcraft III LAN games over the internet"));

    // `?lang=` beats the browser.
    let forced = get("/privacy?lang=es", Some("en")).await.unwrap();
    assert_eq!(forced.headers()["content-language"], "es");
    let privacy = forced.text().await.unwrap();
    assert!(privacy.contains("Política de privacidad") && privacy.contains("Argon2id"));
    assert_eq!(get("/terms", None).await.unwrap().status(), 200);

    // For crawlers.
    let robots = get("/robots.txt", None).await.unwrap().text().await.unwrap();
    assert!(
        robots.contains("Disallow: /auth/") && robots.contains("sitemap.xml"),
        "{robots}"
    );
    let sitemap = get("/sitemap.xml", None).await.unwrap().text().await.unwrap();
    assert!(
        sitemap.contains("/privacy") && sitemap.contains("hreflang=\"en\""),
        "{sitemap}"
    );

    // Static files carry their type and are cacheable.
    let css = get("/assets/style.css", None).await.unwrap();
    assert_eq!(css.headers()["content-type"], "text/css; charset=utf-8");
    assert!(css.headers()["cache-control"].to_str().unwrap().contains("max-age"));
    assert_eq!(
        get("/favicon.svg", None).await.unwrap().headers()["content-type"],
        "image/svg+xml"
    );
    let png = get("/assets/social-preview.png", None).await.unwrap();
    assert_eq!(png.headers()["content-type"], "image/png");
    assert_eq!(&png.bytes().await.unwrap()[..4], b"\x89PNG");

    // The API did not move.
    assert_eq!(get("/health", None).await.unwrap().status(), 200);
}

/// A stand-in for Google's token endpoint: the "code" it is given carries the
/// identity it answers with, as `sub|email|verified|name`.
async fn fake_google() -> String {
    use axum::response::IntoResponse;
    use axum::routing::post;
    use std::collections::HashMap;

    async fn token(
        axum::extract::Form(form): axum::extract::Form<HashMap<String, String>>,
    ) -> axum::response::Response {
        let field = |name: &str| form.get(name).map(String::as_str).unwrap_or("");
        let parts: Vec<&str> = field("code").split('|').collect();
        let ok = field("client_id") == "test-client"
            && field("client_secret") == "test-secret"
            && field("grant_type") == "authorization_code"
            && field("redirect_uri").ends_with("/auth/google/callback");
        if !ok || parts.len() != 4 {
            return (axum::http::StatusCode::BAD_REQUEST, "bad").into_response();
        }
        let enc = |v: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let claims = json!({
            "iss": "https://accounts.google.com", "aud": "test-client", "sub": parts[0],
            "exp": now + 600, "email": parts[1], "email_verified": parts[2] == "true", "name": parts[3],
        });
        let id_token = format!("{}.{}.sig", enc(b"{}"), enc(claims.to_string().as_bytes()));
        axum::Json(json!({ "id_token": id_token })).into_response()
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new().route("/token", post(token));
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}")
}

/// A PKCE verifier and its challenge.
fn pkce() -> (String, String) {
    use sha2::{Digest, Sha256};
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Uuid::new_v4().as_bytes().repeat(2));
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

const APP_STATE: &str = "app-state-0123456789";

/// The player's browser, which never follows redirects: the test plays the
/// parts of Google and of the app's loopback listener itself.
struct Browser<'a> {
    s: &'a Server,
    http: reqwest::Client,
}

impl<'a> Browser<'a> {
    fn new(s: &'a Server) -> Self {
        Self {
            s,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
        }
    }

    /// The app opens `/auth/google/start`; returns the cookie and the `state`
    /// that goes to Google.
    async fn start(&self, challenge: &str) -> (String, String) {
        let res = self
            .http
            .get(format!(
                "{}/auth/google/start?port=51234&state={APP_STATE}&challenge={challenge}",
                self.s.base
            ))
            .send()
            .await
            .unwrap();
        assert!(res.status().is_redirection(), "{}", res.status());
        let location = res.headers()["location"].to_str().unwrap().to_string();
        assert!(location.contains("client_id=test-client") && location.contains("scope=openid+email+profile"));
        let nonce = url::Url::parse(&location)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .to_string();
        let cookie = res.headers()["set-cookie"].to_str().unwrap();
        assert!(
            cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"),
            "{cookie}"
        );
        (cookie.split(';').next().unwrap().to_string(), nonce)
    }

    /// Google sends the browser back; `code` is the identity, for the fake.
    async fn callback(&self, cookie: &str, nonce: &str, code: &str) -> reqwest::Response {
        self.http
            .get(format!("{}/auth/google/callback", self.s.base))
            .query(&[("code", code), ("state", nonce)])
            .header("cookie", cookie)
            .send()
            .await
            .unwrap()
    }

    /// A whole sign-in as the given Google identity; returns the answer's redirect.
    async fn sign_in(&self, challenge: &str, identity: &str) -> reqwest::Response {
        let (cookie, nonce) = self.start(challenge).await;
        self.callback(&cookie, &nonce, identity).await
    }
}

/// Where the browser is sent: always the player's own machine.
fn loopback(res: &reqwest::Response) -> url::Url {
    assert!(res.status().is_redirection(), "{}", res.status());
    let url = url::Url::parse(res.headers()["location"].to_str().unwrap()).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert_eq!(url.port(), Some(51234));
    url
}

fn param(url: &url::Url, name: &str) -> Option<String> {
    url.query_pairs().find(|(k, _)| k == name).map(|(_, v)| v.to_string())
}

#[tokio::test]
async fn google_sign_in_creates_an_account_and_returns_to_the_app() {
    let google = fake_google().await;
    let Some(s) = start_with(|settings| {
        settings.google_client_id = "test-client".into();
        settings.google_client_secret = "test-secret".into();
        settings.google_token_url = format!("{google}/token");
        settings.google_auth_url = format!("{google}/auth");
    })
    .await
    else {
        return;
    };
    let browser = Browser::new(&s);
    let exchange = |code: String, verifier: String| {
        s.call(
            "POST",
            "/auth/google/exchange",
            None,
            Some(json!({ "code": code, "verifier": verifier })),
        )
    };

    let (status, providers) = s.call("GET", "/auth/providers", None, None).await;
    assert_eq!((status, &providers["google"]), (200, &json!(true)));

    // A brand-new Google user gets an account without a password.
    let sub = unique("g");
    let email = format!("{sub}@gmail.test");
    let identity = format!("{sub}|{email}|true|Nuevo Jugador");
    let (verifier, challenge) = pkce();
    let back = loopback(&browser.sign_in(&challenge, &identity).await);
    assert_eq!(param(&back, "state").as_deref(), Some(APP_STATE));
    let code = param(&back, "code").expect("a one-time code");
    // Without the right verifier the code is useless, and a wrong try burns it.
    assert_eq!(exchange(code.clone(), pkce().0).await.0, 400);
    assert_eq!(exchange(code, verifier).await.0, 400);

    let (verifier, challenge) = pkce();
    let code = param(&loopback(&browser.sign_in(&challenge, &identity).await), "code").unwrap();
    let (status, session) = exchange(code.clone(), verifier.clone()).await;
    assert_eq!(status, 200, "{session}");
    assert_eq!(session["user"]["email"], json!(email));
    assert_eq!(session["user"]["displayName"], "Nuevo Jugador");
    assert_eq!(session["user"]["passwordSet"], false);
    let google_user = session["user"]["id"].clone();
    let token = session["tokens"]["accessToken"].as_str().unwrap().to_string();
    assert_eq!(exchange(code, verifier).await.0, 400, "a code works once");

    // Signing in again with the same Google account finds the same user.
    let (verifier, challenge) = pkce();
    let again = format!("{sub}|{email}|true|Otro Nombre");
    let code = param(&loopback(&browser.sign_in(&challenge, &again).await), "code").unwrap();
    assert_eq!(exchange(code, verifier).await.1["user"]["id"], google_user);

    // No password yet, so password sign-in cannot work. One can be set without
    // a current password, and then both ways work.
    let login = |password: &'static str| {
        s.call(
            "POST",
            "/auth/login",
            None,
            Some(json!({ "login": email, "password": password })),
        )
    };
    assert_eq!(login("whatever-1").await.0, 401);
    let (status, _) = s
        .call(
            "POST",
            "/me/password",
            Some(&token),
            Some(json!({ "newPassword": "my-new-password" })),
        )
        .await;
    assert_eq!(status, 200);
    let (status, signed_in) = login("my-new-password").await;
    assert_eq!((status, &signed_in["user"]["id"]), (200, &google_user));
    assert_eq!(s.call("GET", "/me", Some(&token), None).await.1["passwordSet"], true);
}

#[tokio::test]
async fn google_links_to_an_existing_account_only_after_its_password() {
    let google = fake_google().await;
    let Some(s) = start_with(|settings| {
        settings.google_client_id = "test-client".into();
        settings.google_client_secret = "test-secret".into();
        settings.google_token_url = format!("{google}/token");
    })
    .await
    else {
        return;
    };
    let browser = Browser::new(&s);
    let existing = s.register("viejo").await;
    let email = format!("{}@test.dev", existing.username);
    let sub = unique("g");
    let identity = format!("{sub}|{email}|true|Viejo");

    let (verifier, challenge) = pkce();
    let (cookie, nonce) = browser.start(&challenge).await;
    let res = browser.callback(&cookie, &nonce, &identity).await;
    assert_eq!(res.status(), 200, "the e-mail is taken: ask for the password");
    // The form may end at the app's loopback address; nothing else is widened.
    let csp = res.headers()["content-security-policy"].to_str().unwrap().to_string();
    assert!(
        csp.contains("form-action 'self' http://127.0.0.1:*") && csp.contains("default-src 'none'"),
        "{csp}"
    );
    let form = res.text().await.unwrap();
    assert!(form.contains(&email) && form.contains("noindex"));
    let link = form
        .split("name=\"link\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_string();
    let submit = |password: &'static str, cookie: String| {
        browser
            .http
            .post(format!("{}/auth/google/link", s.base))
            .header("cookie", cookie)
            .form(&[("link", link.clone()), ("password", password.to_string())])
            .send()
    };
    assert_eq!(submit("wrong-password", cookie.clone()).await.unwrap().status(), 401);
    // Another browser (no cookie) cannot finish it, right password or not.
    assert_eq!(
        submit("supersecret", "wl_oauth=other".into()).await.unwrap().status(),
        400
    );
    let res = submit("supersecret", cookie.clone()).await.unwrap();
    let code = param(&loopback(&res), "code").expect("linked");
    let (_, linked) = s
        .call(
            "POST",
            "/auth/google/exchange",
            None,
            Some(json!({ "code": code, "verifier": verifier })),
        )
        .await;
    assert_eq!(linked["user"]["id"], json!(existing.id.to_string()));
    assert_eq!(linked["user"]["passwordSet"], true);

    // From now on Google alone gets in, and the form is spent.
    let (verifier, challenge) = pkce();
    let code = param(&loopback(&browser.sign_in(&challenge, &identity).await), "code").unwrap();
    let (_, second) = s
        .call(
            "POST",
            "/auth/google/exchange",
            None,
            Some(json!({ "code": code, "verifier": verifier })),
        )
        .await;
    assert_eq!(second["user"]["id"], json!(existing.id.to_string()));
    assert_eq!(submit("supersecret", cookie).await.unwrap().status(), 400);
}

#[tokio::test]
async fn google_sign_in_refuses_what_it_should() {
    let google = fake_google().await;
    let Some(s) = start_with(|settings| {
        settings.google_client_id = "test-client".into();
        settings.google_client_secret = "test-secret".into();
        settings.google_token_url = format!("{google}/token");
    })
    .await
    else {
        return;
    };
    let browser = Browser::new(&s);
    let (_, challenge) = pkce();

    // An e-mail Google has not verified is not proof of anything.
    let unverified = format!("{}|nuevo@x.test|false|X", unique("g"));
    let res = browser.sign_in(&challenge, &unverified).await;
    assert_eq!(param(&loopback(&res), "error").as_deref(), Some("email_not_verified"));

    // The player pressing "cancel" at Google.
    let (cookie, nonce) = browser.start(&challenge).await;
    let res = browser
        .http
        .get(format!(
            "{}/auth/google/callback?error=access_denied&state={nonce}",
            s.base
        ))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(param(&loopback(&res), "error").as_deref(), Some("access_denied"));

    // A callback that was not started in this browser.
    let (_, nonce) = browser.start(&challenge).await;
    assert_eq!(
        browser
            .callback("wl_oauth=forged", &nonce, "x|a@b.test|true|X")
            .await
            .status(),
        400
    );
    assert_eq!(browser.callback("", &nonce, "x|a@b.test|true|X").await.status(), 400);

    // Start parameters: a privileged port, a short state and nothing at all.
    for query in [
        format!("port=80&state={APP_STATE}"),
        "port=51234&state=short".to_string(),
        String::new(),
    ] {
        let res = browser
            .http
            .get(format!("{}/auth/google/start?{query}&challenge={challenge}", s.base))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400, "{query}");
    }
}

#[tokio::test]
async fn google_sign_in_is_off_without_credentials() {
    let Some(s) = start().await else { return };
    let (_, providers) = s.call("GET", "/auth/providers", None, None).await;
    assert_eq!(providers["google"], false);
    let res = s
        .client
        .get(format!(
            "{}/auth/google/start?port=51234&state={APP_STATE}&challenge={}",
            s.base,
            pkce().1
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn deleting_an_account_removes_it_and_what_hung_from_it() {
    let Some(s) = start().await else { return };
    let leaver = s.register("adios").await;
    let friend = s.register("amigo").await;
    s.befriend(&leaver, &friend).await;
    let room = s.ok("POST", "/rooms", &leaver, Some(json!({ "name": "Se va" }))).await;
    let room_id = room["room"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        s.ok("GET", "/friends", &friend, None).await.as_array().unwrap().len(),
        1
    );

    let delete = |confirm: &str, password: Option<&str>| {
        let body = match password {
            Some(p) => json!({ "confirm": confirm, "password": p }),
            None => json!({ "confirm": confirm }),
        };
        s.call("DELETE", "/me", Some(&leaver.token), Some(body))
    };
    // Typing the username and the password are both required.
    assert_eq!(delete("otro", Some("supersecret")).await.0, 400);
    assert_eq!(delete(&leaver.username, Some("wrong-password")).await.0, 400);
    assert_eq!(delete(&leaver.username, None).await.0, 400);
    assert_eq!(
        s.call("GET", "/me", Some(&leaver.token), None).await.0,
        200,
        "still there"
    );

    let (status, body) = delete(&leaver.username.to_uppercase(), Some("supersecret")).await;
    assert_eq!(status, 200, "{body}");

    // The session, the login, the friendship and the room hosted are gone.
    assert_eq!(s.call("GET", "/me", Some(&leaver.token), None).await.0, 401);
    let (status, _) = s
        .call(
            "POST",
            "/auth/login",
            None,
            Some(json!({ "login": leaver.username, "password": "supersecret" })),
        )
        .await;
    assert_eq!(status, 401);
    assert!(s
        .ok("GET", "/friends", &friend, None)
        .await
        .as_array()
        .unwrap()
        .is_empty());
    let (status, _) = s
        .call("GET", &format!("/rooms/{room_id}"), Some(&friend.token), None)
        .await;
    assert_eq!(status, 404);
    let found = s
        .ok("GET", &format!("/users/search?q={}", leaver.username), &friend, None)
        .await;
    assert!(found.as_array().unwrap().is_empty());
    // The name and e-mail can be used again.
    let (status, _) = s
        .call(
            "POST",
            "/auth/register",
            None,
            Some(json!({ "username": leaver.username, "email": format!("{}@test.dev", leaver.username), "password": "supersecret" })),
        )
        .await;
    assert_eq!(status, 200);
}
