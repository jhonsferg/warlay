//! The whole path of the shared desktop audio with real parts: a tone played by
//! another process -> WASAPI process-loopback -> Opus -> a real P2P mesh link
//! -> the other member's mesh -> Opus decode. Ignored (needs an audio device and
//! Windows 11); run on purpose:
//!
//!     cargo test -p desktop mesh_e2e -- --ignored --test-threads=1

use std::sync::Arc;
use std::time::Duration;

use audiopus::coder::Decoder;
use audiopus::{Channels, SampleRate};
use tokio::net::UdpSocket;
use uuid::Uuid;
use warlay::core::UdpRendezvous;
use warlay::engine::{MeshConfig, PeerMesh, PeerMeshHandle};

use super::encoder::{CHANNELS, FRAME_SAMPLES};
use super::wasapi_tests::{bystander, loudest_tone, play, tone_wav};

async fn two_linked_members() -> (Arc<PeerMeshHandle>, Arc<PeerMeshHandle>, Uuid, Uuid) {
    let secret = b"desktop-audio-e2e-secret".to_vec();
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let rendezvous = socket.local_addr().unwrap();
    Arc::new(UdpRendezvous::new(socket).with_ticket_secret(secret.clone())).spawn();

    let room = Uuid::new_v4();
    let member = |user: Uuid, is_host: bool| {
        let ticket = warlay::protocol::RoomTicket {
            room_id: room,
            user_id: user,
            is_host,
            expires_unix: u64::MAX,
        };
        Arc::new(PeerMesh::spawn_with(MeshConfig {
            rendezvous_addr: rendezvous,
            room_token: room,
            own_client_id: user,
            is_host,
            ticket: Some(warlay::crypto::sign_token(&secret, &ticket.encode().unwrap())),
            relay_addr: None,
        }))
    };
    let (sender_id, listener_id) = (Uuid::new_v4(), Uuid::new_v4());
    let sender = member(sender_id, true);
    let listener = member(listener_id, false);
    let start = tokio::time::Instant::now();
    while sender.connected_peers().is_empty() || listener.connected_peers().is_empty() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the two members never linked"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    (sender, listener, sender_id, listener_id)
}

/// Decodes the received Opus packets to interleaved stereo PCM.
fn decode(packets: &[Vec<u8>]) -> Vec<i16> {
    let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo).unwrap();
    let mut pcm = Vec::new();
    for packet in packets {
        let mut frame = vec![0i16; FRAME_SAMPLES];
        if let Ok(n) = decoder.decode(Some(packet.as_slice()), &mut frame[..], false) {
            pcm.extend_from_slice(&frame[..n * CHANNELS]);
        }
    }
    pcm
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs an audio output device and Windows 11 / build 20348+"]
async fn a_tone_played_on_this_pc_reaches_the_other_member_intact() {
    let (sender, listener, sender_id, _) = two_linked_members().await;

    // The player stands for the game; the bystander for the WebView2 (excluded).
    let wav = tone_wav(10, 440.0);
    let mut webview = bystander();
    let mut game = play(&wav);
    let share = super::start(sender.clone(), webview.id()).expect("start sharing the desktop audio");

    // Listen for a few seconds on the other member's side.
    let mut packets = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    while tokio::time::Instant::now() < deadline {
        for (from, frame) in listener.poll_desktop_frames() {
            assert_eq!(from, sender_id, "frames must be attributed to the sharer");
            packets.push(frame);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    drop(share);
    let _ = game.kill();
    let _ = webview.kill();

    assert!(
        packets.len() > 50,
        "only {} packets arrived: the stream is not flowing",
        packets.len()
    );
    assert!(packets.iter().all(|p| p.len() < 1000), "a packet exceeds a datagram");
    let tone = loudest_tone(&decode(&packets), 440.0);
    println!(
        "{} packets reached the listener; 440 Hz amplitude after decoding: {tone:.0}",
        packets.len()
    );
    assert!(
        tone > 8000.0,
        "the tone did not survive capture -> Opus -> mesh -> decode ({tone:.0})"
    );
}
