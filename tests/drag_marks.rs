use star_battle_solver::{
    model::{Board, Game, Mark},
    session::Session,
};

fn session() -> Session {
    let mut game = Game::default();
    game.load(
        Board::new(
            10,
            (0..100).map(|i| i / 10).collect(),
            vec![Mark::Empty; 100],
        )
        .unwrap(),
    );
    game.play(&[(33, Mark::Cross), (44, Mark::Star)]);
    Session::new(game)
}

#[test]
fn a_stroke_preserves_existing_symbols_and_is_one_undoable_action() {
    let mut session = session();
    let before = session.game.board.clone().unwrap();
    session.begin_cross_stroke();
    session.extend_cross_stroke(&[44, 43, 42, 41, 40]); // Start on a star; cross an automatic X.
    session.extend_cross_stroke(&[40, 41, 42, 33, 34]); // Retrace and cross manual/automatic X's.
    assert_eq!(session.painted_cells(), &[42, 41, 40]);
    assert_eq!(session.game.board.as_ref().unwrap(), &before); // Live overlay only until release.
    session.finish_cross_stroke();
    let after = session.game.board.clone().unwrap();
    for i in [40, 41, 42] {
        assert_eq!(after.marks[i], Mark::Cross);
    }
    for i in [33, 34, 43, 44] {
        assert_eq!(after.marks[i], before.marks[i]);
    }
    assert_eq!(after.automatic_crosses, before.automatic_crosses); // Existing automatic X's stay automatic.
    session.game.undo();
    assert_eq!(session.game.board.as_ref().unwrap(), &before);
    session.game.redo();
    assert_eq!(session.game.board.as_ref().unwrap(), &after);
    session.edit(44, Some(Mark::Empty));
    let board = session.game.board.as_ref().unwrap();
    assert_eq!(board.marks[43], Mark::Empty);
    for i in [33, 40, 41, 42] {
        assert_eq!(board.marks[i], Mark::Cross);
    }
}

#[test]
fn cancellation_or_an_intervening_edit_discards_the_stroke() {
    let mut session = session();
    let before = session.game.board.clone();
    session.begin_cross_stroke();
    session.extend_cross_stroke(&[0, 1, 2]);
    session.cancel_cross_stroke();
    session.finish_cross_stroke();
    assert_eq!(session.game.board, before);
    assert!(session.painted_cells().is_empty());
    session.begin_cross_stroke();
    session.extend_cross_stroke(&[0, 1, 2]);
    session.edit(0, Some(Mark::Star));
    session.finish_cross_stroke();
    assert_eq!(session.game.board.as_ref().unwrap().marks[0], Mark::Star);
    assert!(
        !session
            .game
            .board
            .as_ref()
            .unwrap()
            .automatic_crosses
            .contains(&2)
    );
    assert_eq!(session.game.board.as_ref().unwrap().marks[2], Mark::Empty);
}

#[test]
fn drag_over_only_existing_symbols_does_not_create_history() {
    let mut session = session();
    session.begin_cross_stroke();
    session.extend_cross_stroke(&[33, 34, 44, 55]);
    session.finish_cross_stroke();
    session.game.undo();
    assert!(
        session
            .game
            .board
            .as_ref()
            .unwrap()
            .marks
            .iter()
            .all(|&m| m == Mark::Empty)
    );
}
