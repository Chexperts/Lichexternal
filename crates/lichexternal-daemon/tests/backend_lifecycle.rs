//! Integration tests for engine backend lifecycle, commands, and cancellation.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use lichexternal_core::uci::{parse_uci_line, UciOutputLine};
use lichexternal_daemon::backend::{BackendError, EngineBackend};

/// In-memory mock engine backend to test protocol interactions deterministically.
pub struct MockEngineBackend {
    incoming_commands: Vec<String>,
    outgoing_lines: Arc<Mutex<VecDeque<String>>>,
    alive: Arc<AtomicBool>,
    calculating: Arc<AtomicBool>,
}

impl MockEngineBackend {
    pub fn new() -> Self {
        Self {
            incoming_commands: Vec::new(),
            outgoing_lines: Arc::new(Mutex::new(VecDeque::new())),
            alive: Arc::new(AtomicBool::new(true)),
            calculating: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn received_commands(&self) -> &[String] {
        &self.incoming_commands
    }
}

impl Default for MockEngineBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineBackend for MockEngineBackend {
    async fn send_command(&mut self, cmd: &str) -> Result<(), BackendError> {
        let trimmed = cmd.trim();
        self.incoming_commands.push(trimmed.to_string());

        let mut lines = self.outgoing_lines.lock().await;

        match trimmed {
            "uci" => {
                lines.push_back("id name MockEngine 1.0".into());
                lines.push_back("id author Chexperts".into());
                lines.push_back("uciok".into());
            }
            "isready" => {
                lines.push_back("readyok".into());
            }
            "ucinewgame" => {}
            s if s.starts_with("position") => {}
            s if s.starts_with("go") => {
                self.calculating.store(true, Ordering::SeqCst);
                lines.push_back("info depth 1 score cp 20 nodes 10 pv e2e4".into());
                lines.push_back("info depth 2 score cp 35 nodes 50 pv e2e4 e7e5".into());
                lines.push_back("info depth 3 score cp 40 nodes 120 pv e2e4 e7e5 g1f3".into());
            }
            "stop" => {
                if self.calculating.swap(false, Ordering::SeqCst) {
                    lines.push_back("bestmove e2e4 ponder e7e5".into());
                }
            }
            "quit" => {
                self.alive.store(false, Ordering::SeqCst);
            }
            _ => {}
        }

        Ok(())
    }

    async fn read_line(&mut self) -> Result<Option<String>, BackendError> {
        if !self.alive.load(Ordering::SeqCst) {
            return Ok(None);
        }

        let mut lines = self.outgoing_lines.lock().await;
        Ok(lines.pop_front())
    }

    fn is_alive(&mut self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

#[tokio::test]
async fn test_mock_backend_handshake() {
    let mut backend = MockEngineBackend::new();

    backend.send_command("uci").await.unwrap();

    let mut responses = Vec::new();
    while let Some(line) = backend.read_line().await.unwrap() {
        responses.push(line);
    }

    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0], "id name MockEngine 1.0");
    assert_eq!(responses[1], "id author Chexperts");
    assert_eq!(responses[2], "uciok");

    backend.send_command("isready").await.unwrap();
    let ready = backend.read_line().await.unwrap();
    assert_eq!(ready.as_deref(), Some("readyok"));
    assert!(backend.is_alive());
}

#[tokio::test]
async fn test_mock_backend_calculation_and_cancellation() {
    let mut backend = MockEngineBackend::new();

    // Start calculation
    backend.send_command("go depth 20").await.unwrap();

    let line1 = backend.read_line().await.unwrap().unwrap();
    let parsed1 = parse_uci_line(&line1);
    match parsed1 {
        UciOutputLine::Info(info) => {
            assert_eq!(info.depth, Some(1));
            assert_eq!(info.pv, vec!["e2e4"]);
        }
        _ => panic!("Expected UciOutputLine::Info"),
    }

    let line2 = backend.read_line().await.unwrap().unwrap();
    assert!(line2.starts_with("info depth 2"));

    let line3 = backend.read_line().await.unwrap().unwrap();
    assert!(line3.starts_with("info depth 3"));

    // Now simulate cancellation (e.g. client disconnects on Lichess web)
    backend.stop().await.unwrap();

    let bestmove_line = backend.read_line().await.unwrap().unwrap();
    match parse_uci_line(&bestmove_line) {
        UciOutputLine::BestMove { best_move, ponder } => {
            assert_eq!(best_move, "e2e4");
            assert_eq!(ponder, Some("e7e5"));
        }
        _ => panic!("Expected UciOutputLine::BestMove"),
    }

    // Engine must remain alive after stop!
    assert!(backend.is_alive());

    // Subsequent commands succeed without restarting the process
    backend.send_command("isready").await.unwrap();
    let ready = backend.read_line().await.unwrap();
    assert_eq!(ready.as_deref(), Some("readyok"));

    assert_eq!(
        backend.received_commands(),
        &["go depth 20", "stop", "isready"]
    );
}

#[tokio::test]
async fn test_stdio_backend_with_echo_process() {
    // Test StdioBackend with standard system binary 'cat' or 'sh'
    let mut backend = lichexternal_daemon::backend::StdioBackend::spawn("cat", &[]).unwrap();
    assert!(backend.is_alive());

    backend.send_command("isready").await.unwrap();
    let line = backend.read_line().await.unwrap();
    assert_eq!(line.as_deref(), Some("isready"));

    backend.send_command("stop").await.unwrap();
    let stop_line = backend.read_line().await.unwrap();
    assert_eq!(stop_line.as_deref(), Some("stop"));

    backend.kill().await.unwrap();
}
