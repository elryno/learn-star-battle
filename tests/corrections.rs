mod common;
use common::expected;
use star_battle_solver::{
    model::{Board, Game, Mark},
    session::Session,
    solver::{Advice, SearchResult, advise, solve},
};
use std::time::Duration;

// These tests check correctness, not interactive latency. Leave headroom for
// unoptimized 14x14 searches on shared CI runners; timeout behavior is tested
// separately with Duration::ZERO. The app keeps its own eight-second limit.
const BUDGET: Duration = Duration::from_secs(60);

fn solution(board: &Board) -> Vec<Mark> {
    let SearchResult::Solution(rows) = solve(board, BUDGET) else {
        panic!("expected a solution")
    };
    (0..board.marks.len())
        .map(|i| {
            if rows[i / board.size] & (1 << (i % board.size)) != 0 {
                Mark::Star
            } else {
                Mark::Cross
            }
        })
        .collect()
}

#[test]
fn incorrect_stars_and_xs_get_proven_corrections_in_every_size() {
    for name in ["5x5-empty", "8x8-empty", "10x10-empty", "14x14-empty"] {
        let empty = expected(name);
        let solved = solution(&empty);
        for wrong in [Mark::Cross, Mark::Star] {
            let cell = solved.iter().position(|&m| m != wrong).unwrap();
            let mut game = Game::default();
            game.load(empty.clone());
            game.play(&[(cell, wrong)]);
            let before = game.board.clone().unwrap();
            let result = advise(&before, BUDGET);
            let Advice::Correction(step) = &result else {
                panic!("{name}: {result:?}")
            };
            assert_eq!(step.focus, vec![cell]);
            assert_eq!(step.changes, vec![(cell, solved[cell])]);
            assert!(step.clue.contains(&before.address(cell)));
            assert!(step.clue.contains(if wrong == Mark::Star {
                "to an X"
            } else {
                "to a star"
            }));
            let mut session = Session::new(game);
            session.receive(0, result, false);
            assert!(!session.can_request_hint());
            assert_eq!(session.conflict_cells(&before), vec![cell]);
            assert!(session.message_error);
            assert_eq!(session.game.board.as_ref().unwrap(), &before);
            session.apply_hint(); // A hint alone never changes the board.
            assert_eq!(session.game.board.as_ref().unwrap(), &before);
            session.reveal(true);
            assert!(!session.can_request_hint());
            session.apply_hint();
            assert!(session.can_request_hint());
            assert!(session.error_cells.is_empty());
            let after = session.game.board.as_ref().unwrap();
            assert_eq!(after.marks[cell], solved[cell]);
            assert!(
                after
                    .marks
                    .iter()
                    .zip(&solved)
                    .all(|(&mark, &answer)| mark == Mark::Empty || mark == answer),
                "{name}: the known valid completion must satisfy every corrected/automatic mark"
            );
            session.game.undo();
            assert_eq!(session.game.board.as_ref().unwrap(), &before);
        }
    }
}

#[test]
fn several_wrong_marks_can_be_corrected_one_hint_at_a_time() {
    let mut board = expected("5x5-empty");
    let solved = solution(&board);
    let stars: Vec<_> = solved
        .iter()
        .enumerate()
        .filter(|(_, m)| **m == Mark::Star)
        .take(2)
        .map(|(i, _)| i)
        .collect();
    for &i in &stars {
        board.marks[i] = Mark::Cross;
    }
    for _ in 0..2 {
        let Advice::Correction(step) = advise(&board, BUDGET) else {
            panic!("expected correction")
        };
        assert_eq!(step.changes.len(), 1);
        assert!(stars.contains(&step.changes[0].0));
        board.play_marks(&step.changes, true);
    }
    assert!(matches!(solve(&board, BUDGET), SearchResult::Solution(_)));
}

#[test]
fn ambiguous_layout_repairs_are_explicitly_conditional_on_other_marks() {
    let mut board = Board::new(5, (0..25).map(|i| i / 5).collect(), vec![Mark::Empty; 25]).unwrap();
    board.marks[0] = Mark::Star;
    board.marks[2] = Mark::Star;
    let Advice::Correction(step) = advise(&board, BUDGET) else {
        panic!("expected a repair")
    };
    assert!(step.clue.contains("Keeping your other placed marks"));
    let cell = step.changes[0].0;
    assert!([0, 2].contains(&cell));
    let mut alone = board.clone();
    alone.marks.fill(Mark::Empty);
    alone.marks[cell] = Mark::Star;
    assert!(matches!(solve(&alone, BUDGET), SearchResult::Solution(_)));
    board.play_marks(&step.changes, true);
    assert!(matches!(solve(&board, BUDGET), SearchResult::Solution(_)));
}

#[test]
fn ambiguous_multiple_errors_highlight_only_a_proven_conflicting_group() {
    let mut board = Board::new(5, (0..25).map(|i| i / 5).collect(), vec![Mark::Empty; 25]).unwrap();
    for i in [0, 2, 10, 12] {
        board.marks[i] = Mark::Star;
    }
    let Advice::Contradiction { message, cells } = advise(&board, BUDGET) else {
        panic!("expected a group")
    };
    assert!(message.contains("at least one"));
    assert!(cells.len() >= 2);
    assert!(cells.iter().all(|&i| board.marks[i] == Mark::Star));
    let mut core = board.clone();
    core.marks.fill(Mark::Empty);
    for &i in &cells {
        core.marks[i] = board.marks[i];
    }
    assert!(matches!(solve(&core, BUDGET), SearchResult::Impossible));
    for &i in &cells {
        core.marks[i] = Mark::Empty;
        assert!(matches!(solve(&core, BUDGET), SearchResult::Solution(_)));
        core.marks[i] = board.marks[i];
    }
}

#[test]
fn impossible_layout_and_timeouts_do_not_blame_symbols() {
    let mut regions = vec![2; 25];
    regions[0] = 0;
    regions[1] = 1;
    regions[15..20].fill(3);
    regions[20..].fill(4);
    let mut board = Board::new(5, regions, vec![Mark::Empty; 25]).unwrap();
    board.marks[24] = Mark::Cross;
    let Advice::Contradiction { message, cells } = advise(&board, BUDGET) else {
        panic!("expected layout error")
    };
    assert!(message.contains("region layout"));
    assert!(cells.is_empty());
    board = expected("5x5-empty");
    board.marks[0] = Mark::Star;
    board.marks[1] = Mark::Star;
    let Advice::Contradiction { cells, .. } = advise(&board, Duration::ZERO) else {
        panic!("expected bounded diagnosis")
    };
    assert!(cells.is_empty());
}

#[test]
fn edits_clear_correction_highlights_and_discard_old_diagnoses() {
    let mut board = expected("5x5-empty");
    let cell = solution(&board)
        .iter()
        .position(|&m| m == Mark::Star)
        .unwrap();
    board.marks[cell] = Mark::Cross;
    let advice = advise(&board, BUDGET);
    assert!(matches!(advice, Advice::Correction(_)));
    let mut game = Game::default();
    game.load(board);
    let mut session = Session::new(game);
    session.receive(0, advice.clone(), false);
    assert_eq!(session.error_cells, vec![cell]);
    session.edit(cell, Some(Mark::Empty));
    session.receive(0, advice, false);
    assert!(session.error_cells.is_empty());
    assert!(session.advice.is_none());
}
