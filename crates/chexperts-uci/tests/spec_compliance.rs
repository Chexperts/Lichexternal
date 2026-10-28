//! Exhaustive specification compliance test suite for the Universal Chess Interface (UCI).
//!
//! Validates compliance against all GUI-to-Engine commands and Engine-to-GUI responses
//! defined in the official Stefan Meyer-Kahlen & Rudolf Huber UCI protocol standard.

use chexperts_uci::command::{GoParams, PositionInitial, RegisterCommand, UciCommand};
use chexperts_uci::info::{ScoreBound, UciScore};
use chexperts_uci::output::{
    parse_uci_line, CopyProtectionStatus, OptionType, RegistrationStatus, UciOutputLine,
};

// ============================================================================
// 1. GUI to Engine Command Formatting Tests
// ============================================================================

#[test]
fn test_gui_handshake_and_synchronization_commands() {
    assert_eq!(UciCommand::Uci.to_uci_line(), "uci");
    assert_eq!(UciCommand::IsReady.to_uci_line(), "isready");
    assert_eq!(UciCommand::UciNewGame.to_uci_line(), "ucinewgame");
    assert_eq!(UciCommand::Stop.to_uci_line(), "stop");
    assert_eq!(UciCommand::PonderHit.to_uci_line(), "ponderhit");
    assert_eq!(UciCommand::Quit.to_uci_line(), "quit");
}

#[test]
fn test_gui_debug_command() {
    assert_eq!(UciCommand::Debug(true).to_uci_line(), "debug on");
    assert_eq!(UciCommand::Debug(false).to_uci_line(), "debug off");
}

#[test]
fn test_gui_setoption_command() {
    assert_eq!(
        UciCommand::set_option("Threads", Some("8")).to_uci_line(),
        "setoption name Threads value 8"
    );

    assert_eq!(
        UciCommand::set_option("Clear Hash", None).to_uci_line(),
        "setoption name Clear Hash"
    );
}

#[test]
fn test_gui_register_command() {
    assert_eq!(
        UciCommand::Register(RegisterCommand::Later).to_uci_line(),
        "register later"
    );

    assert_eq!(
        UciCommand::Register(RegisterCommand::Credentials {
            name: "Chexperts Team".into(),
            code: "KEY-12345-XYZ".into(),
        })
        .to_uci_line(),
        "register name Chexperts Team code KEY-12345-XYZ"
    );
}

#[test]
fn test_gui_position_command() {
    assert_eq!(
        UciCommand::Position {
            initial: PositionInitial::Startpos,
            moves: vec![],
        }
        .to_uci_line(),
        "position startpos"
    );

    assert_eq!(
        UciCommand::Position {
            initial: PositionInitial::Startpos,
            moves: vec!["e2e4".into(), "e7e5".into(), "g1f3".into()],
        }
        .to_uci_line(),
        "position startpos moves e2e4 e7e5 g1f3"
    );

    assert_eq!(
        UciCommand::Position {
            initial: PositionInitial::fen(
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
            ),
            moves: vec!["c7c5".into()],
        }
        .to_uci_line(),
        "position fen rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1 moves c7c5"
    );
}

#[test]
fn test_gui_go_command_variants() {
    // Default infinite
    assert_eq!(
        UciCommand::Go(GoParams::default()).to_uci_line(),
        "go infinite"
    );

    // Classical time control with increment
    let tc = GoParams {
        wtime: Some(300_000),
        btime: Some(280_000),
        winc: Some(3_000),
        binc: Some(3_000),
        movestogo: Some(40),
        ..Default::default()
    };
    assert_eq!(
        UciCommand::Go(tc).to_uci_line(),
        "go wtime 300000 btime 280000 winc 3000 binc 3000 movestogo 40"
    );

    // Fixed depth and movetime
    let fixed = GoParams {
        depth: Some(20),
        movetime: Some(5_000),
        nodes: Some(10_000_000),
        ..Default::default()
    };
    assert_eq!(
        UciCommand::Go(fixed).to_uci_line(),
        "go depth 20 nodes 10000000 movetime 5000"
    );

    // Searchmoves restriction & ponder mode
    let searchmoves = GoParams {
        searchmoves: vec!["e2e4".into(), "d2d4".into()],
        ponder: true,
        mate: Some(5),
        ..Default::default()
    };
    assert_eq!(
        UciCommand::Go(searchmoves).to_uci_line(),
        "go searchmoves e2e4 d2d4 ponder mate 5"
    );
}

// ============================================================================
// 2. Engine to GUI Output Line Parsing Tests
// ============================================================================

#[test]
fn test_engine_handshake_responses() {
    assert_eq!(parse_uci_line("uciok\r\n"), UciOutputLine::UciOk);
    assert_eq!(parse_uci_line("readyok\n"), UciOutputLine::ReadyOk);
    assert_eq!(
        parse_uci_line("id name Stockfish 17"),
        UciOutputLine::IdName("Stockfish 17")
    );
    assert_eq!(
        parse_uci_line("id author the Stockfish developers"),
        UciOutputLine::IdAuthor("the Stockfish developers")
    );
}

#[test]
fn test_engine_bestmove_parsing() {
    assert_eq!(
        parse_uci_line("bestmove e2e4"),
        UciOutputLine::BestMove {
            best_move: "e2e4",
            ponder: None,
        }
    );

    assert_eq!(
        parse_uci_line("bestmove e7e8q ponder e1e2\r\n"),
        UciOutputLine::BestMove {
            best_move: "e7e8q",
            ponder: Some("e1e2"),
        }
    );

    assert_eq!(
        parse_uci_line("bestmove 0000"),
        UciOutputLine::BestMove {
            best_move: "0000",
            ponder: None,
        }
    );
}

#[test]
fn test_engine_copyprotection_and_registration() {
    assert_eq!(
        parse_uci_line("copyprotection checking"),
        UciOutputLine::CopyProtection(CopyProtectionStatus::Checking)
    );
    assert_eq!(
        parse_uci_line("copyprotection ok"),
        UciOutputLine::CopyProtection(CopyProtectionStatus::Ok)
    );
    assert_eq!(
        parse_uci_line("copyprotection error"),
        UciOutputLine::CopyProtection(CopyProtectionStatus::Error)
    );

    assert_eq!(
        parse_uci_line("registration checking"),
        UciOutputLine::Registration(RegistrationStatus::Checking)
    );
    assert_eq!(
        parse_uci_line("registration ok"),
        UciOutputLine::Registration(RegistrationStatus::Ok)
    );
    assert_eq!(
        parse_uci_line("registration error"),
        UciOutputLine::Registration(RegistrationStatus::Error)
    );
}

#[test]
fn test_engine_option_declarations() {
    let check_opt = "option name Nullmove type check default true";
    if let UciOutputLine::Option(opt) = parse_uci_line(check_opt) {
        assert_eq!(opt.name, "Nullmove");
        assert_eq!(opt.opt_type, OptionType::Check);
        assert_eq!(opt.default, Some("true"));
    } else {
        panic!("Failed to parse check option");
    }

    let spin_opt = "option name Hash type spin default 16 min 1 max 1048576";
    if let UciOutputLine::Option(opt) = parse_uci_line(spin_opt) {
        assert_eq!(opt.name, "Hash");
        assert_eq!(opt.opt_type, OptionType::Spin);
        assert_eq!(opt.default, Some("16"));
        assert_eq!(opt.min, Some(1));
        assert_eq!(opt.max, Some(1048576));
    } else {
        panic!("Failed to parse spin option");
    }

    let combo_opt = "option name Style type combo default Normal var Solid var Normal var Risky";
    if let UciOutputLine::Option(opt) = parse_uci_line(combo_opt) {
        assert_eq!(opt.name, "Style");
        assert_eq!(opt.opt_type, OptionType::Combo);
        assert_eq!(opt.default, Some("Normal"));
        assert_eq!(opt.vars, vec!["Solid", "Normal", "Risky"]);
    } else {
        panic!("Failed to parse combo option");
    }

    let button_opt = "option name Clear Hash type button";
    if let UciOutputLine::Option(opt) = parse_uci_line(button_opt) {
        assert_eq!(opt.name, "Clear Hash");
        assert_eq!(opt.opt_type, OptionType::Button);
    } else {
        panic!("Failed to parse button option");
    }
}

#[test]
fn test_engine_info_comprehensive_tokens() {
    let line = "info depth 28 seldepth 42 multipv 2 score cp 142 lowerbound nodes 34567890 nps 4500000 time 7680 hashfull 720 tbhits 1200 sbhits 450 cpuload 980 currmove e2e4 currmovenumber 1 pv e2e4 c7c5 g1f3 d7d6";
    let parsed = parse_uci_line(line);

    match parsed {
        UciOutputLine::Info(info) => {
            assert_eq!(info.depth, Some(28));
            assert_eq!(info.seldepth, Some(42));
            assert_eq!(info.multipv, Some(2));
            assert_eq!(info.score, Some(UciScore::Cp(142)));
            assert_eq!(info.score_bound, Some(ScoreBound::LowerBound));
            assert_eq!(info.nodes, Some(34567890));
            assert_eq!(info.nps, Some(4500000));
            assert_eq!(info.time_ms, Some(7680));
            assert_eq!(info.hashfull, Some(720));
            assert_eq!(info.tbhits, Some(1200));
            assert_eq!(info.sbhits, Some(450));
            assert_eq!(info.cpuload, Some(980));
            assert_eq!(info.currmove, Some("e2e4"));
            assert_eq!(info.currmovenumber, Some(1));
            assert_eq!(info.pv, vec!["e2e4", "c7c5", "g1f3", "d7d6"]);
            assert_eq!(info.raw, line);
        }
        _ => panic!("Expected UciOutputLine::Info"),
    }
}

#[test]
fn test_engine_info_mate_and_upperbound() {
    let line =
        "info depth 14 seldepth 18 score mate -4 upperbound nodes 12345 time 45 pv d1h5 g7g6";
    let parsed = parse_uci_line(line);

    match parsed {
        UciOutputLine::Info(info) => {
            assert_eq!(info.score, Some(UciScore::Mate(-4)));
            assert_eq!(info.score_bound, Some(ScoreBound::UpperBound));
            assert_eq!(info.pv, vec!["d1h5", "g7g6"]);
        }
        _ => panic!("Expected UciOutputLine::Info"),
    }
}

#[test]
fn test_engine_info_string() {
    let line = "info string NNUE evaluation loaded successfully";
    let parsed = parse_uci_line(line);

    match parsed {
        UciOutputLine::Info(info) => {
            assert_eq!(info.string, Some("NNUE evaluation loaded successfully"));
        }
        _ => panic!("Expected UciOutputLine::Info with string payload"),
    }
}
