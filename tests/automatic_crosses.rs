use star_battle_solver::{
    model::{Board, Game, Mark},
    session::Session,
    solver::{Advice, Step},
};

fn game(size: usize) -> Game {
    let mut game = Game::default();
    game.load(
        Board::new(
            size,
            (0..size * size).map(|i| i / size).collect(),
            vec![Mark::Empty; size * size],
        )
        .unwrap(),
    );
    game
}

#[test]
fn removing_a_star_clears_its_automatic_neighbors_but_keeps_manual_xs() {
    let mut game = game(10);
    game.play(&[(33, Mark::Cross)]);
    game.play(&[(44, Mark::Star)]);
    let board = game.board.as_ref().unwrap();
    for i in [33, 34, 35, 43, 45, 53, 54, 55] {
        assert_eq!(board.marks[i], Mark::Cross);
    }
    assert_eq!(board.automatic_crosses, vec![34, 35, 43, 45, 53, 54, 55]);
    game.play(&[(44, Mark::Empty)]);
    let mut expected = vec![Mark::Empty; 100];
    expected[33] = Mark::Cross;
    assert_eq!(game.board.as_ref().unwrap().marks, expected);
    assert!(game.board.unwrap().automatic_crosses.is_empty());
}

#[test]
fn shared_neighbors_and_completed_rows_recalculate_after_removal() {
    let mut game = game(10);
    game.play(&[(44, Mark::Star), (46, Mark::Star)]);
    assert_eq!(game.board.as_ref().unwrap().marks[40], Mark::Cross);
    game.play(&[(44, Mark::Empty)]);
    let board = game.board.unwrap();
    assert_eq!(board.marks[45], Mark::Cross); // Still adjacent to the other star.
    assert_eq!(board.marks[35], Mark::Cross);
    assert_eq!(board.marks[33], Mark::Empty); // Only the removed star required it.
    assert_eq!(board.marks[40], Mark::Empty); // The row is no longer full.
    assert_eq!(board.marks[49], Mark::Empty);
    assert_eq!(board.marks[46], Mark::Star);
}

#[test]
fn completed_column_xs_clear_when_its_quota_is_no_longer_met() {
    let mut game = game(10);
    game.play(&[(44, Mark::Star), (64, Mark::Star)]);
    assert_eq!(game.board.as_ref().unwrap().marks[4], Mark::Cross);
    game.play(&[(44, Mark::Empty)]);
    let board = game.board.unwrap();
    assert_eq!(board.marks[4], Mark::Empty);
    assert_eq!(board.marks[54], Mark::Cross); // Shared neighbor still needed by G5.
    assert_eq!(board.marks[64], Mark::Star);
}

#[test]
fn rows_and_columns_fill_at_the_correct_quota_for_every_size() {
    for size in [5, 8, 10, 14] {
        for column in [false, true] {
            let mut game = game(size);
            let quota = game.board.as_ref().unwrap().required();
            let far = if column { (size - 1) * size } else { size - 1 };
            for star in 0..quota {
                assert_eq!(game.board.as_ref().unwrap().marks[far], Mark::Empty);
                let i = if column { star * 2 * size } else { star * 2 };
                game.play(&[(i, Mark::Star)]);
            }
            assert_eq!(game.board.as_ref().unwrap().marks[far], Mark::Cross);
        }
    }
}

#[test]
fn regions_fill_at_the_correct_quota_independently_of_rows_and_columns() {
    for size in [5, 8, 10, 14] {
        // Region 0 is an L along the top and left edges; the other regions
        // are the remaining cells of each row.
        let mut game = Game::default();
        game.load(
            Board::new(
                size,
                (0..size * size)
                    .map(|i| if i % size == 0 { 0 } else { i / size })
                    .collect(),
                vec![Mark::Empty; size * size],
            )
            .unwrap(),
        );
        let quota = game.board.as_ref().unwrap().required();
        let far = size - 1;
        for star in 0..quota {
            assert_eq!(game.board.as_ref().unwrap().marks[far], Mark::Empty);
            let i = if star == 0 {
                (size - 1) * size
            } else {
                star * 2
            };
            game.play(&[(i, Mark::Star)]);
        }
        assert_eq!(game.board.as_ref().unwrap().marks[far], Mark::Cross);
        assert!(
            game.board
                .as_ref()
                .unwrap()
                .automatic_crosses
                .contains(&far)
        );
        game.undo();
        assert_eq!(game.board.as_ref().unwrap().marks[far], Mark::Empty);
        game.redo();
        assert_eq!(game.board.as_ref().unwrap().marks[far], Mark::Cross);
        game.set_auto_crosses(false);
        assert_eq!(game.board.as_ref().unwrap().marks[far], Mark::Empty);
        game.set_auto_crosses(true);
        assert_eq!(game.board.as_ref().unwrap().marks[far], Mark::Cross);
    }
}

fn irregular_region_game() -> Game {
    let mut board: Board =
        serde_json::from_str(include_str!("fixtures/10x10-reported-position.json")).unwrap();
    board.marks.fill(Mark::Empty);
    let mut game = Game::default();
    game.load(board);
    game
}

#[test]
fn removing_a_region_star_clears_region_xs_but_keeps_manual_and_neighbor_marks() {
    let mut game = irregular_region_game();
    game.play(&[(84, Mark::Cross), (75, Mark::Star)]); // I5 manual; H6 star.
    assert_eq!(game.board.as_ref().unwrap().marks[87], Mark::Empty);
    game.play(&[(95, Mark::Star)]); // J6 completes the green region.
    let board = game.board.as_ref().unwrap();
    assert_eq!(board.marks[87], Mark::Cross); // I8 is not a star neighbor.
    assert!(board.automatic_crosses.contains(&87));
    assert!(!board.automatic_crosses.contains(&84));
    game.play(&[(75, Mark::Empty)]);
    let board = game.board.as_ref().unwrap();
    assert_eq!(board.marks[87], Mark::Empty);
    assert_eq!(board.marks[84], Mark::Cross); // Manual I5 stays.
    assert_eq!(board.marks[94], Mark::Cross); // J5 still touches J6.
}

#[test]
fn region_x_shared_with_a_completed_column_stays_until_both_are_incomplete() {
    let mut game = irregular_region_game();
    game.play(&[(75, Mark::Star), (95, Mark::Star)]);
    game.play(&[(17, Mark::Star), (37, Mark::Star)]); // Complete column 8.
    game.play(&[(75, Mark::Empty)]); // The green region is no longer complete.
    assert_eq!(game.board.as_ref().unwrap().marks[87], Mark::Cross);
    game.play(&[(17, Mark::Empty)]); // Neither constraint now excludes I8.
    assert_eq!(game.board.as_ref().unwrap().marks[87], Mark::Empty);
}

#[test]
fn explicit_x_promotes_an_automatic_mark_to_a_manual_note() {
    let mut game = game(10);
    game.play(&[(44, Mark::Star)]);
    game.play(&[(35, Mark::Cross)]);
    assert!(!game.board.as_ref().unwrap().automatic_crosses.contains(&35));
    game.play(&[(44, Mark::Empty)]);
    assert_eq!(game.board.as_ref().unwrap().marks[35], Mark::Cross);
    assert!(game.board.as_ref().unwrap().automatic_crosses.is_empty());
}

#[test]
fn automatic_xs_never_replace_conflicting_stars() {
    let mut game = game(10);
    game.play(&[(0, Mark::Star), (1, Mark::Star), (4, Mark::Star)]);
    let board = game.board.unwrap();
    for i in [0, 1, 4] {
        assert_eq!(board.marks[i], Mark::Star);
    }
    assert_eq!(board.marks[9], Mark::Empty); // An overfull row has not met its quota.
    assert!(
        board
            .violations()
            .iter()
            .any(|v| v.message.contains("touch"))
    );
    assert!(
        board
            .violations()
            .iter()
            .any(|v| v.message.starts_with("Row A has 3"))
    );
}

#[test]
fn toggle_undo_and_redo_keep_manual_marks_and_current_preference() {
    let mut game = game(10);
    game.play(&[(33, Mark::Cross)]);
    game.play(&[(44, Mark::Star)]);
    let after = game.board.clone();
    game.undo();
    assert_eq!(game.board.as_ref().unwrap().marks[44], Mark::Empty);
    assert_eq!(game.board.as_ref().unwrap().marks[33], Mark::Cross);
    game.redo();
    assert_eq!(game.board, after);
    game.set_auto_crosses(false);
    assert!(game.board.as_ref().unwrap().automatic_crosses.is_empty());
    assert_eq!(game.board.as_ref().unwrap().marks[33], Mark::Cross);
    game.undo(); // Toggling did not create a board-history entry.
    assert_eq!(game.board.as_ref().unwrap().marks[44], Mark::Empty);
    game.redo();
    assert_eq!(game.board.as_ref().unwrap().marks[44], Mark::Star);
    assert_eq!(game.board.as_ref().unwrap().marks[34], Mark::Empty);
    assert!(!game.auto_crosses);
    game.set_auto_crosses(true);
    assert_eq!(game.board.as_ref().unwrap().marks[34], Mark::Cross);
    game.undo();
    assert!(game.auto_crosses);
    assert_eq!(game.board.as_ref().unwrap().marks[34], Mark::Empty);
}

fn star_step() -> Advice {
    Advice::Step(Step {
        clue: "clue".into(),
        explanation: "reason".into(),
        focus: vec![44],
        changes: vec![(44, Mark::Star)],
        premises: vec![],
        frames: vec![],
    })
}

#[test]
fn previews_include_automatic_xs_without_turning_them_into_manual_marks() {
    let mut session = Session::new(game(10));
    session.receive(0, star_step(), true);
    let preview = session.targets().to_vec();
    assert_eq!(preview.len(), 9);
    assert_eq!(
        session.game.board.as_ref().unwrap().marks,
        vec![Mark::Empty; 100]
    );
    session.apply_hint();
    assert!(session.targets().is_empty());
    assert!(session.message.is_empty());
    let board = session.game.board.as_ref().unwrap();
    for &(i, mark) in &preview {
        assert_eq!(board.marks[i], mark);
    }
    assert_eq!(board.automatic_crosses.len(), 8);
    session.edit(44, Some(Mark::Empty));
    assert_eq!(
        session.game.board.as_ref().unwrap().marks,
        vec![Mark::Empty; 100]
    );
    session.game.undo();
    assert_eq!(
        session.game.board.as_ref().unwrap().automatic_crosses.len(),
        8
    );
    session.game.undo();
    assert_eq!(
        session.game.board.as_ref().unwrap().marks,
        vec![Mark::Empty; 100]
    );
}

#[test]
fn changing_the_setting_invalidates_previews_and_late_analysis() {
    let mut session = Session::new(game(10));
    session.receive(0, star_step(), true);
    session.set_auto_crosses(false);
    session.receive(0, star_step(), true);
    assert!(!session.preview);
    assert_eq!(session.targets(), &[(44, Mark::Star)]);
    assert!(session.advice.is_none()); // The late result was discarded.
    assert_eq!(session.game.board.as_ref().unwrap().marks[44], Mark::Empty);
    assert_eq!(session.targets(), &[(44, Mark::Star)]);
    session.apply_hint();
    assert_eq!(
        session
            .game
            .board
            .as_ref()
            .unwrap()
            .marks
            .iter()
            .filter(|&&m| m == Mark::Cross)
            .count(),
        0
    );
}

#[test]
fn retained_proposals_recompute_automatic_xs_and_skip_completed_marks() {
    let mut session = Session::new(game(10));
    session.receive(0, star_step(), true);
    assert_eq!(session.targets().len(), 9);
    session.edit(99, Some(Mark::Cross));
    assert_eq!(session.targets().len(), 9);
    session.edit(33, Some(Mark::Cross));
    assert_eq!(session.targets().len(), 8);
    assert!(!session.targets().iter().any(|&(i, _)| i == 33));
    session.set_auto_crosses(false);
    assert_eq!(session.targets(), &[(44, Mark::Star)]);
    session.set_auto_crosses(true);
    assert_eq!(session.targets().len(), 8);
    session.edit(44, Some(Mark::Star));
    assert!(session.targets().is_empty());
    assert!(!session.has_hint());
    assert_eq!(
        session.game.board.as_ref().unwrap().automatic_crosses.len(),
        7
    );
}

#[test]
fn legacy_saves_default_to_enabled_without_claiming_manual_xs() {
    let mut game = game(10);
    game.set_auto_crosses(false);
    game.play(&[(33, Mark::Cross), (44, Mark::Star)]);
    let mut old = serde_json::to_value(&game).unwrap();
    old.as_object_mut().unwrap().remove("auto_crosses");
    old.as_object_mut().unwrap().remove("show_errors");
    old["board"]
        .as_object_mut()
        .unwrap()
        .remove("automatic_crosses");
    let mut loaded: Game = serde_json::from_value(old).unwrap();
    assert!(loaded.auto_crosses);
    assert!(loaded.show_errors);
    assert!(loaded.valid());
    loaded.refresh_automatic_crosses();
    loaded.play(&[(44, Mark::Empty)]);
    assert_eq!(loaded.board.as_ref().unwrap().marks[33], Mark::Cross);
    assert!(loaded.board.unwrap().automatic_crosses.is_empty());
}
