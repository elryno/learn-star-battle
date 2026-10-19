use star_battle_solver::{
    model::{Board, Game, Mark},
    session::Session,
    solver::{Advice, advise},
};
use std::time::Duration;

fn trial() -> Session {
    let board: Board =
        serde_json::from_str(include_str!("fixtures/10x10-daily-page.json")).unwrap();
    let advice = advise(&board, Duration::from_secs(8));
    let Advice::Step(step) = &advice else {
        panic!("{advice:?}");
    };
    assert_eq!(step.changes, vec![(62, Mark::Cross)]);
    assert_eq!(step.frames.len(), 4);
    assert_eq!(step.frames[0].marks[62], Mark::Star);
    assert_eq!(step.frames[0].focus, vec![62]);
    assert_eq!(
        step.frames[2].focus,
        (0..10).map(|r| r * 10 + 2).collect::<Vec<_>>()
    );
    for i in [2, 12, 22, 92] {
        assert_eq!(step.frames[2].marks[i], Mark::Cross);
    }
    assert_eq!(
        step.frames[3].conflicts,
        vec![2, 3, 4, 12, 13, 14, 22, 23, 24]
    );
    let mut game = Game::default();
    game.load(board);
    let mut session = Session::new(game);
    assert!(session.can_request_hint());
    session.receive(session.revision, advice, false);
    assert!(!session.can_request_hint());
    session
}

#[test]
fn tutorial_navigation_is_read_only_and_final_apply_is_one_real_move() {
    let mut s = trial();
    let original = serde_json::to_string(&s.game).unwrap();
    s.start_explanation();
    for index in 0..4 {
        assert_eq!(s.lesson_index, Some(index));
        assert!(s.lesson_frame().is_some());
        assert!(!s.can_request_hint());
        assert!(!s.can_apply_hint());
        assert!(s.targets().is_empty());
        s.edit(62, Some(Mark::Star));
        s.begin_cross_stroke();
        s.extend_cross_stroke(&[0, 1]);
        s.finish_cross_stroke();
        s.apply_hint();
        assert_eq!(serde_json::to_string(&s.game).unwrap(), original);
        s.navigate_explanation(true);
    }
    assert!(s.lesson_frame().is_none());
    assert!(s.can_apply_hint());
    assert!(!s.can_request_hint());
    assert_eq!(s.targets(), &[(62, Mark::Cross)]);
    s.navigate_explanation(false);
    assert!(!s.lesson_frame().unwrap().conflicts.is_empty());
    assert!(s.targets().is_empty());
    s.navigate_explanation(true);
    assert_eq!(serde_json::to_string(&s.game).unwrap(), original);
    s.apply_hint();
    assert_eq!(s.game.board.as_ref().unwrap().marks[62], Mark::Cross);
    assert!(!s.has_hint());
    assert!(s.can_request_hint());
    assert!(s.lesson_index.is_none());
    assert!(s.focus.is_empty() && s.targets().is_empty() && s.message.is_empty());
    s.game.undo();
    assert_eq!(s.game.board.as_ref().unwrap().marks[62], Mark::Empty);
    assert!(!s.game.can_undo());
}

#[test]
fn exiting_trial_restores_play_and_manual_completion_clears_lesson() {
    let mut s = trial();
    let original = s.game.board.clone();
    s.start_explanation();
    s.navigate_explanation(true);
    s.leave_explanation();
    assert!(s.lesson_index.is_none());
    assert!(s.has_hint());
    assert!(!s.can_request_hint());
    assert!(!s.can_apply_hint());
    assert_eq!(s.game.board, original);
    s.start_explanation();
    for _ in 0..4 {
        s.navigate_explanation(true);
    }
    s.edit(62, Some(Mark::Cross));
    assert!(!s.has_hint());
    assert!(s.lesson_index.is_none());
    assert!(!s.can_apply_hint());
    assert!(s.message.is_empty());
    assert!(s.can_request_hint());
}

#[test]
fn local_hint_can_apply_only_its_remaining_targets_after_manual_progress() {
    let board: Board =
        serde_json::from_str(include_str!("fixtures/10x10-reported-position.json")).unwrap();
    let advice = advise(&board, Duration::from_secs(8));
    let mut game = Game::default();
    game.set_auto_crosses(false);
    game.load(board);
    let mut s = Session::new(game);
    s.receive(s.revision, advice, false);
    s.start_explanation();
    assert!(s.lesson_index.is_none());
    assert!(s.can_apply_hint());
    assert_eq!(s.targets(), &[(80, Mark::Cross), (90, Mark::Cross)]);
    s.edit(80, Some(Mark::Cross));
    assert!(s.can_apply_hint());
    assert!(!s.can_request_hint());
    assert_eq!(s.targets(), &[(90, Mark::Cross)]);
    s.apply_hint();
    assert!(!s.has_hint());
    s.game.undo();
    assert_eq!(s.game.board.as_ref().unwrap().marks[80], Mark::Cross);
    assert_eq!(s.game.board.as_ref().unwrap().marks[90], Mark::Empty);
}

#[test]
fn clicking_through_x_to_a_suggested_star_keeps_the_hint_until_complete() {
    let mut board = Board::new(5, (0..25).map(|i| i / 5).collect(), vec![Mark::Empty; 25]).unwrap();
    board.marks[1..5].fill(Mark::Cross);
    let advice = advise(&board, Duration::from_secs(5));
    let mut game = Game::default();
    game.set_auto_crosses(false);
    game.load(board);
    let mut s = Session::new(game);
    s.receive(s.revision, advice, false);
    s.start_explanation();
    assert_eq!(s.targets(), &[(0, Mark::Star)]);
    s.edit(0, None); // Empty -> X is the first of two clicks to place a star.
    assert!(s.has_hint());
    assert!(s.can_apply_hint());
    assert_eq!(s.targets(), &[(0, Mark::Star)]);
    s.edit(0, None);
    assert_eq!(s.game.board.as_ref().unwrap().marks[0], Mark::Star);
    assert!(!s.has_hint());
    assert!(s.targets().is_empty());
}
