mod common;
use common::expected;
use star_battle_solver::{
    model::{Board, Game, Mark, star_count},
    solver::{Advice, SearchResult, advise, solve},
};
use std::time::Duration;

#[test]
fn infers_only_the_site_formats() {
    assert_eq!(
        [5, 8, 10, 14].map(star_count),
        [Some(1), Some(1), Some(2), Some(3)]
    );
    assert_eq!(star_count(6), None);
}

#[test]
fn finds_valid_completions_for_all_sizes() {
    for name in ["5x5-empty", "8x8-empty", "10x10-empty", "14x14-empty"] {
        let mut board = expected(name);
        let SearchResult::Solution(rows) = solve(&board, Duration::from_secs(10)) else {
            panic!("could not solve {name}")
        };
        for (i, m) in board.marks.iter_mut().enumerate() {
            *m = if rows[i / board.size] & (1 << (i % board.size)) != 0 {
                Mark::Star
            } else {
                Mark::Empty
            };
        }
        assert!(board.solved(), "{name}: {:?}", board.violations());
    }
}

#[test]
fn repeated_dynamic_steps_finish_each_size_without_overwriting_moves() {
    for name in ["5x5-empty", "8x8-empty", "10x10-empty", "14x14-empty"] {
        let mut board = expected(name);
        let mut steps = 0;
        while !board.solved() {
            let advice = advise(&board, Duration::from_secs(10));
            let Advice::Step(step) = advice else {
                panic!("{name} at step {steps}: {advice:?}")
            };
            assert!(!step.explanation.is_empty());
            assert!(!step.changes.is_empty());
            assert!(!step.explanation.starts_with("Contradiction proof"));
            assert!(!step.explanation.starts_with("After checking"));
            for (i, mark) in step.changes {
                assert_eq!(board.marks[i], Mark::Empty);
                board.marks[i] = mark;
            }
            assert!(board.violations().is_empty());
            steps += 1;
            assert!(steps <= board.size * board.size);
        }
        eprintln!("{name} completed in {steps} deductions");
    }
}

#[test]
fn highlights_direct_conflicts_but_not_unfinished_units() {
    let mut board = expected("10x10-empty");
    assert!(board.violations().is_empty());
    board.marks[0] = Mark::Star;
    board.marks[11] = Mark::Star;
    assert!(
        board
            .violations()
            .iter()
            .any(|v| v.cells == vec![0, 11] && v.message.contains("touch"))
    );
    board.marks.fill(Mark::Empty);
    for i in 0..10 {
        board.marks[i] = Mark::Cross;
    }
    assert!(
        board
            .violations()
            .iter()
            .any(|v| v.message.contains("Row A") && v.message.contains("space"))
    );
}

#[test]
fn distant_diagonals_are_allowed_and_two_stars_may_share_a_unit() {
    let mut board = expected("10x10-empty");
    board.marks[0] = Mark::Star;
    board.marks[22] = Mark::Star;
    assert!(
        !board
            .violations()
            .iter()
            .any(|v| v.message.contains("touch"))
    );
    board.marks[22] = Mark::Empty;
    board.marks[2] = Mark::Star;
    assert!(
        !board
            .violations()
            .iter()
            .any(|v| v.message.contains("Row A") && v.message.contains("Remove"))
    );
    board.marks[4] = Mark::Star;
    assert!(
        board
            .violations()
            .iter()
            .any(|v| v.message.contains("Row A") && v.message.contains("Remove"))
    );
}

fn reported_position() -> Board {
    serde_json::from_str(include_str!("fixtures/10x10-reported-position.json")).unwrap()
}

#[test]
fn reported_complex_hint_uses_visible_counts_before_a_trial() {
    let board: Board =
        serde_json::from_str(include_str!("fixtures/10x10-counting-position.json")).unwrap();
    let Advice::Step(step) = advise(&board, Duration::from_secs(8)) else {
        panic!("expected a deduction");
    };
    assert!(
        step.frames.is_empty(),
        "expected a visible counting hint: {step:?}"
    );
    assert_eq!(
        step.changes,
        vec![(60, Mark::Cross), (62, Mark::Cross), (63, Mark::Cross)]
    );
    assert!(step.explanation.contains("Rows H–J"));
    assert!(step.explanation.contains("6 stars"));
    assert!(step.explanation.contains("G1, G3, G4"));
    for (cell, _) in step.changes {
        let mut opposite = board.clone();
        opposite.marks[cell] = Mark::Star;
        assert!(matches!(
            solve(&opposite, Duration::from_secs(2)),
            SearchResult::Impossible
        ));
    }
}

#[test]
fn a_single_placement_beats_a_smaller_group_with_multiple_cases() {
    let mut board = Board::new(
        10,
        (0..100).map(|i| i / 10).collect(),
        vec![Mark::Empty; 100],
    )
    .unwrap();
    board.play_marks(&[(0, Mark::Star)], true);
    // Row A offers two alternatives and a common-neighbor elimination.
    for i in (2..10).filter(|i| ![3, 4].contains(i)) {
        board.marks[i] = Mark::Cross;
    }
    // Row J has more empty cells, but only one way to fit its two stars.
    for i in (90..100).filter(|i| ![91, 92, 93].contains(i)) {
        board.marks[i] = Mark::Cross;
    }
    let Advice::Step(step) = advise(&board, Duration::from_secs(5)) else {
        panic!("expected a deduction");
    };
    assert_eq!(step.changes, vec![(91, Mark::Star), (93, Mark::Star)]);
    assert!(step.explanation.contains("Row J"));
    assert!(step.explanation.contains("J2, J4"));
    assert!(step.frames.is_empty());

    // An immediately visible count still takes priority over those alternatives.
    board.marks[92] = Mark::Cross;
    let Advice::Step(step) = advise(&board, Duration::from_secs(5)) else {
        panic!("expected a direct count");
    };
    assert_eq!(step.changes, vec![(91, Mark::Star), (93, Mark::Star)]);
    assert!(step.explanation.contains("exactly 2 cells available"));
}

#[test]
fn reported_position_gets_a_local_pair_deduction_before_a_distant_star() {
    let board = reported_position();
    let Advice::Step(step) = advise(&board, Duration::from_secs(8)) else {
        panic!("expected a deduction");
    };
    assert_eq!(step.changes, vec![(80, Mark::Cross), (90, Mark::Cross)]);
    assert!(step.clue.contains("I2") && step.clue.contains("J2"));
    assert!(step.explanation.contains("I1, J1"));
    assert_eq!(step.focus, vec![81, 82, 83, 91]);
    assert!(step.frames.is_empty());
    for (i, _) in step.changes {
        let mut opposite = board.clone();
        opposite.marks[i] = Mark::Star;
        assert!(matches!(
            solve(&opposite, Duration::from_secs(2)),
            SearchResult::Impossible
        ));
    }
    // Once I9 is present, its unmarked neighbors are a simpler deduction.
    let mut after = board;
    after.marks[88] = Mark::Star;
    let Advice::Step(step) = advise(&after, Duration::from_secs(8)) else {
        panic!();
    };
    assert!(step.clue.contains("I9"));
    assert!(step.explanation.contains("cannot touch"));
    assert!(
        step.changes
            .iter()
            .all(|&(i, m)| after.neighbors(88).contains(&i) && m == Mark::Cross)
    );
}

#[test]
fn count_errors_cover_the_entire_unit_and_keep_other_conflicts_separate() {
    let mut board = reported_position();
    board.marks[80] = Mark::Star;
    board.marks[88] = Mark::Star;
    let violations = board.violations();
    assert_eq!(violations[0].cells, (80..90).collect::<Vec<_>>());
    assert_eq!(
        violations[0].message,
        "Row I has 3 stars; it needs exactly 2. Remove 1 star."
    );
    assert!(violations.iter().any(|v| v.cells == vec![81, 82, 83, 91]));
    board = reported_position();
    board.marks.fill(Mark::Empty);
    for i in [0, 20, 40] {
        board.marks[i] = Mark::Star;
    }
    assert!(
        board
            .violations()
            .iter()
            .any(|v| v.message.starts_with("Column 1")
                && v.cells == (0..10).map(|r| r * 10).collect::<Vec<_>>())
    );
    board.marks.fill(Mark::Empty);
    for i in [0, 2, 4] {
        board.marks[i] = Mark::Star;
    }
    let region: Vec<_> = (0..100).filter(|&i| board.regions[i] == 0).collect();
    assert!(
        board
            .violations()
            .iter()
            .any(|v| v.message.starts_with("Region 1 ") && v.cells == region)
    );
}

#[test]
fn global_contradictions_and_time_limits_are_distinct() {
    let board = expected("5x5-empty");
    assert!(matches!(solve(&board, Duration::ZERO), SearchResult::Limit));
    let SearchResult::Solution(rows) = solve(&board, Duration::from_secs(2)) else {
        panic!()
    };
    let mut wrong = board.clone();
    let star = rows[0].trailing_zeros() as usize;
    wrong.marks[star] = Mark::Cross;
    assert!(wrong.violations().is_empty());
    assert!(matches!(
        advise(&wrong, Duration::from_secs(2)),
        Advice::Correction(_)
    ));
}

#[test]
fn ambiguous_layout_never_gets_an_arbitrary_solution_move() {
    let board = Board::new(5, (0..25).map(|i| i / 5).collect(), vec![Mark::Empty; 25]).unwrap();
    assert!(matches!(
        advise(&board, Duration::from_secs(2)),
        Advice::NoForcedMove
    ));
}

#[test]
fn undo_redo_and_persistence_preserve_complete_actions() {
    let board = expected("5x5-empty");
    let mut game = Game::default();
    game.load(board.clone());
    game.change(|b| {
        b.marks[0] = Mark::Star;
        b.marks[1] = Mark::Cross;
    });
    let after = game.board.clone();
    game.undo();
    assert_eq!(game.board, Some(board));
    game.redo();
    assert_eq!(game.board, after);
    let encoded = serde_json::to_string(&game).unwrap();
    let mut restored: Game = serde_json::from_str(&encoded).unwrap();
    assert!(restored.valid());
    restored.undo();
    assert_eq!(restored.board.unwrap().marks[0], Mark::Empty);
    game.undo();
    game.change(|b| b.marks[2] = Mark::Cross);
    assert!(!game.can_redo());
}

#[test]
fn malformed_and_disconnected_regions_are_rejected() {
    let mut board = expected("5x5-empty");
    board.regions[24] = 99;
    assert!(board.validate().is_err());
    board = expected("5x5-empty");
    board.regions[24] = board.regions[0];
    assert!(board.validate().is_err());
    board = expected("5x5-empty");
    board.region_colors.pop();
    assert!(board.validate().is_err());
    board.region_colors.clear(); // Older saves have no palette.
    assert!(board.validate().is_ok());
}
