//! End-to-end integration test of the EngineSession supervisor.
//!
//! Spawns a real subprocess mock engine via StdioBackend, connects to a wiremock
//! broker server, processes a work request, streams evaluations, and shuts down cleanly.

use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use lichexternal_core::config::{BackendType, EngineConfig};
use lichexternal_core::models::{AcquireResponse, SearchLimit, UciVariant, WorkRequest};
use lichexternal_daemon::broker::BrokerClient;
use lichexternal_daemon::supervisor::{EngineSession, SessionState};

/// Custom responder that returns a work item on the first call, and 204 on subsequent calls.
struct PollWorkResponder {
    call_count: Arc<AtomicUsize>,
    work_item: AcquireResponse,
}

impl Respond for PollWorkResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);
        if count == 0 {
            ResponseTemplate::new(200).set_body_json(&self.work_item)
        } else {
            ResponseTemplate::new(204)
        }
    }
}

#[tokio::test]
#[cfg(unix)]
async fn test_supervisor_e2e_work_cycle_and_shutdown() {
    use std::os::unix::fs::PermissionsExt;

    // 1. Create a mock engine script
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let script_path = temp_dir.path().join("mock_engine.sh");
    let script_content = r#"#!/bin/sh
while IFS= read -r line; do
    case "$line" in
        "uci")
            echo "id name MockEngine 2.0"
            echo "id author Chexperts"
            echo "uciok"
            ;;
        "isready")
            echo "readyok"
            ;;
        "ucinewgame")
            ;;
        position*)
            ;;
        go*)
            echo "info depth 1 score cp 25 pv e2e4"
            echo "info depth 2 score cp 40 pv e2e4 e7e5"
            echo "bestmove e2e4 ponder e7e5"
            ;;
        "stop")
            echo "bestmove e2e4 ponder e7e5"
            ;;
        "quit")
            exit 0
            ;;
    esac
done
"#;
    fs::write(&script_path, script_content).expect("Failed to write mock engine script");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("set permissions");

    // 2. Start wiremock broker server
    let mock_server = MockServer::start().await;

    let work_job = AcquireResponse {
        id: "e2e-job-101".into(),
        work: WorkRequest {
            session_id: "sess-1".into(),
            threads: 1,
            hash: 16,
            multi_pv: 1,
            variant: UciVariant::Chess,
            initial_fen: None,
            moves: vec!["e2e4".into()],
            search_limit: SearchLimit {
                depth: Some(10),
                movetime: None,
                nodes: None,
            },
        },
        engine: None,
    };

    let poll_count = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/api/external-engine/work"))
        .respond_with(PollWorkResponder {
            call_count: poll_count.clone(),
            work_item: work_job,
        })
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/external-engine/work/e2e-job-101"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    // 3. Configure and start EngineSession
    let config = EngineConfig {
        id: "e2e-engine-id".into(),
        provider_secret: "secret-e2e".into(),
        name: "E2E Engine".into(),
        backend_type: BackendType::Stdio,
        command: Some(script_path.to_str().unwrap().into()),
        args: vec![],
        socket: None,
        max_threads: 1,
        max_hash: 16,
        variants: vec![UciVariant::Chess],
        uci_options: BTreeMap::new(),
    };

    let broker_client = BrokerClient::new(mock_server.uri());
    let (mut session, mut state_rx) = EngineSession::new(config, broker_client);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let handle = tokio::spawn(async move {
        session.run(shutdown_rx).await;
    });

    // 4. Assert state transitions: Initializing -> Idle -> Analyzing -> Idle
    let mut saw_idle = false;
    let mut saw_analyzing = false;
    let mut saw_idle_after_work = false;

    let timeout_duration = Duration::from_secs(10);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout_duration {
        if state_rx.changed().await.is_ok() {
            let state = state_rx.borrow().clone();
            match state {
                SessionState::Idle if !saw_analyzing => {
                    saw_idle = true;
                }
                SessionState::Analyzing { ref work_id, .. } => {
                    assert_eq!(work_id, "e2e-job-101");
                    saw_analyzing = true;
                }
                SessionState::Idle if saw_analyzing => {
                    saw_idle_after_work = true;
                    break;
                }
                SessionState::Error(err) => {
                    panic!("Encountered unexpected engine session error: {err}");
                }
                _ => {}
            }
        }
    }

    assert!(saw_idle, "Engine session should have reached Idle state");
    assert!(
        saw_analyzing,
        "Engine session should have reached Analyzing state"
    );
    assert!(
        saw_idle_after_work,
        "Engine session should have returned to Idle after finishing work"
    );

    // Wait briefly for at least one subsequent poll to occur while in Idle
    let poll_start = std::time::Instant::now();
    while poll_start.elapsed() < Duration::from_secs(5) {
        if poll_count.load(Ordering::SeqCst) >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // 5. Trigger clean shutdown and wait for exit
    shutdown_tx.send(true).expect("Failed to send shutdown");
    handle.await.expect("Session task panicked");

    assert_eq!(*state_rx.borrow(), SessionState::Stopped);
    assert!(
        poll_count.load(Ordering::SeqCst) >= 2,
        "Expected at least 2 polls (1 work + >=1 subsequent poll)"
    );
}
