//! End-to-end integration test with a mock Lichess broker server.

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use lichexternal_core::models::{AcquireResponse, SearchLimit, UciVariant, WorkRequest};
use lichexternal_daemon::broker::BrokerClient;

#[tokio::test]
async fn test_broker_poll_timeout_204() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/external-engine/work"))
        .and(body_json(serde_json::json!({
            "providerSecret": "secret-abc"
        })))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock_server)
        .await;

    let client = BrokerClient::new(mock_server.uri());
    let poll_res = client.poll_work("secret-abc").await.unwrap();
    assert_eq!(poll_res, None);
}

#[tokio::test]
async fn test_broker_poll_and_chunked_stream_e2e() {
    let mock_server = MockServer::start().await;

    let expected_job = AcquireResponse {
        id: "work-job-999".into(),
        work: WorkRequest {
            session_id: "session-abc".into(),
            threads: 4,
            hash: 256,
            multi_pv: 1,
            variant: UciVariant::Chess,
            initial_fen: None,
            moves: vec!["e2e4".into(), "c7c5".into()],
            search_limit: SearchLimit {
                depth: Some(15),
                movetime: None,
                nodes: None,
            },
        },
        engine: None,
    };

    // 1. Mock work acquisition endpoint
    Mock::given(method("POST"))
        .and(path("/api/external-engine/work"))
        .and(body_json(serde_json::json!({
            "providerSecret": "secret-xyz"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(&expected_job))
        .mount(&mock_server)
        .await;

    // 2. Mock work result streaming endpoint
    Mock::given(method("POST"))
        .and(path("/api/external-engine/work/work-job-999"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let client = BrokerClient::new(mock_server.uri());

    // Acquire work
    let job = client
        .poll_work("secret-xyz")
        .await
        .unwrap()
        .expect("Expected work item");
    assert_eq!(job.id, "work-job-999");
    assert_eq!(job.work.threads, 4);
    assert_eq!(job.work.moves, vec!["e2e4", "c7c5"]);

    // Stream lines
    let mut stream = client.start_work_stream(&job.id);
    assert!(stream.is_stream_active());

    stream
        .send_line("info depth 1 score cp 15 pv g1f3")
        .await
        .unwrap();
    stream
        .send_line("info depth 2 score cp 20 pv g1f3 d7d6")
        .await
        .unwrap();
    stream.send_line("bestmove g1f3 ponder d7d6").await.unwrap();

    let status = stream.finish().await.unwrap();
    assert_eq!(status, reqwest::StatusCode::OK);
}
