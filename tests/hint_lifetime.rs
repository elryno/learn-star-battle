use star_battle_solver::{
    model::{Board, Game, Mark},
    session::Session,
    solver::{Advice, advise},
};
use std::time::Duration;

fn session() -> Session {
    let board: Board =
        serde_json::from_str(include_str!("fixtures/10x10-reported-position.json")).unwrap();
    let mut game = Game::default();
    game.set_auto_crosses(false);
    game.load(board);
    Session::new(game)
}

fn hint(session: &mut Session, preview: bool) {
    let advice = advise(session.game.board.as_ref().unwrap(), Duration::from_secs(5));
    assert!(matches!(advice, Advice::Step(_) | Advice::Correction(_)));
    session.receive(session.revision, advice, preview);
}

#[test]
fn hint_survives_unrelated_edits_and_partial_progress_until_all_targets_are_done() {
    let mut s = session();
    hint(&mut s, false);
    let message = s.message.clone();
    let focus = s.focus.clone();
    assert!(!focus.is_empty());
    assert_eq!(
        s.advice.as_ref().unwrap().changes,
        vec![(80, Mark::Cross), (90, Mark::Cross)]
    );
    s.edit(99, Some(Mark::Cross));
    s.edit(0, Some(Mark::Empty)); // Removing an unrelated note does not discard this hint.
    assert!(s.has_hint());
    assert_eq!(s.message, message);
    assert_eq!(s.focus, focus);
    assert!(s.advice.is_none()); // Text is retained, not an executable stale step.
    assert!(s.targets().is_empty()); // Unrevealed hints do not expose their answer.
    s.edit(80, Some(Mark::Cross));
    assert_eq!(s.message, message);
    assert_eq!(s.focus, focus);
    s.edit(90, Some(Mark::Cross));
    assert!(!s.has_hint());
    assert!(s.focus.is_empty());
    assert_ne!(s.message, message);
}

#[test]
fn revealed_targets_survive_edits_and_disappear_one_at_a_time() {
    let mut s = session();
    hint(&mut s, true);
    let message = s.message.clone();
    let original = s.targets().to_vec();
    let focus = s.focus.clone();
    s.edit(99, Some(Mark::Cross));
    assert_eq!(s.targets(), original);
    assert_eq!(s.focus, focus);
    assert!(s.has_proposed_marks());
    assert!(!s.preview);
    assert!(s.advice.is_none());
    assert!(s.can_apply_hint()); // Remaining targets are rebuilt for the current board.
    assert_eq!(s.game.board.as_ref().unwrap().marks[80], Mark::Empty);

    s.edit(80, Some(Mark::Cross));
    assert_eq!(s.message, message);
    assert_eq!(s.targets(), &[(90, Mark::Cross)]);
    assert_eq!(s.focus, focus);
    assert_eq!(s.game.board.as_ref().unwrap().marks[80], Mark::Cross);
    s.game.undo();
    s.changed();
    assert_eq!(s.targets(), original);
    assert_eq!(s.focus, focus);
    s.game.redo();
    s.changed();
    assert_eq!(s.targets(), &[(90, Mark::Cross)]);
    assert_eq!(s.focus, focus);
    s.edit(90, Some(Mark::Cross));
    assert!(!s.has_hint());
    assert!(s.focus.is_empty());
    assert!(!s.has_proposed_marks());
    assert!(s.targets().is_empty());
}

#[test]
fn revealed_correction_and_its_explanation_stay_until_fixed() {
    let mut s = session();
    s.edit(80, Some(Mark::Star));
    hint(&mut s, true);
    let message = s.message.clone();
    assert_eq!(s.targets(), &[(80, Mark::Cross)]);
    s.edit(99, Some(Mark::Cross));
    assert_eq!(s.message, message);
    assert_eq!(s.error_cells, vec![80]);
    assert_eq!(s.targets(), &[(80, Mark::Cross)]);
    s.edit(80, Some(Mark::Cross));
    assert!(s.targets().is_empty());
    assert!(!s.has_hint());
    assert!(s.focus.is_empty());
    assert!(s.error_cells.is_empty());
}

#[test]
fn changing_a_supporting_fact_discards_the_hint_and_old_results_stay_invalid() {
    let mut s = session();
    let old = advise(s.game.board.as_ref().unwrap(), Duration::from_secs(5));
    s.receive(0, old.clone(), true);
    let message = s.message.clone();
    s.edit(83, Some(Mark::Empty));
    s.receive(0, old, false);
    assert!(!s.has_hint());
    assert!(s.focus.is_empty());
    assert_ne!(s.message, message);
    assert!(s.targets().is_empty());
}

#[test]
fn drag_start_cancel_and_partial_strokes_keep_the_hint_and_highlights() {
    let mut s = session();
    hint(&mut s, true);
    let message = s.message.clone();
    let original = s.targets().to_vec();
    let focus = s.focus.clone();
    s.begin_cross_stroke();
    assert!(!s.preview);
    assert_eq!(s.focus, focus);
    assert_eq!(s.message, message);
    assert_eq!(s.targets(), original);
    s.extend_cross_stroke(&[80]);
    s.cancel_cross_stroke();
    assert_eq!(s.message, message);
    assert_eq!(s.focus, focus);
    assert_eq!(s.targets(), original);
    s.begin_cross_stroke();
    s.extend_cross_stroke(&[80]);
    s.finish_cross_stroke();
    assert_eq!(s.message, message);
    assert_eq!(s.focus, focus);
    assert_eq!(s.targets(), &[(90, Mark::Cross)]);
    s.begin_cross_stroke();
    s.extend_cross_stroke(&[90]);
    s.finish_cross_stroke();
    assert!(!s.has_hint());
    assert!(s.focus.is_empty());
    assert!(s.targets().is_empty());
}

#[test]
fn unconditional_correction_stays_until_the_bad_symbol_is_changed() {
    let mut s = session();
    s.game.set_auto_crosses(true);
    s.edit(80, Some(Mark::Star));
    hint(&mut s, false);
    let message = s.message.clone();
    s.edit(0, Some(Mark::Empty));
    assert_eq!(s.message, message);
    assert_eq!(s.error_cells, vec![80]);
    assert!(s.has_hint());
    assert!(s.message_error);
    s.set_auto_crosses(false);
    assert_eq!(s.message, message);
    assert_eq!(s.error_cells, vec![80]);
    s.edit(80, Some(Mark::Empty));
    assert!(!s.has_hint());
    assert!(s.error_cells.is_empty());
    assert!(s.focus.is_empty());
    assert!(!s.message_error);
}

#[test]
fn conditional_correction_expires_when_the_other_conflicting_mark_is_removed() {
    let mut game = Game::default();
    game.set_auto_crosses(false);
    game.load(Board::new(5, (0..25).map(|i| i / 5).collect(), vec![Mark::Empty; 25]).unwrap());
    game.play(&[(0, Mark::Star), (2, Mark::Star)]);
    let mut s = Session::new(game);
    hint(&mut s, false);
    let target = s.advice.as_ref().unwrap().changes[0].0;
    let other = if target == 0 { 2 } else { 0 };
    s.edit(other, Some(Mark::Empty));
    assert!(!s.has_hint());
    assert_eq!(s.game.board.as_ref().unwrap().marks[target], Mark::Star);
}

#[test]
fn selecting_a_conflict_and_undoing_an_unrelated_move_keep_the_hint() {
    let mut s = session();
    s.edit(80, Some(Mark::Star));
    hint(&mut s, false);
    let message = s.message.clone();
    s.select_conflict(0); // Inspecting a rule conflict takes priority over the hint.
    assert_eq!(s.message, message);
    assert!(s.error_cells.is_empty());
    s.edit(99, Some(Mark::Cross));
    assert_eq!(s.error_cells, vec![80]);
    s.game.undo();
    s.changed();
    assert_eq!(s.message, message);
    assert_eq!(s.error_cells, vec![80]);
    s.game.redo();
    s.changed();
    assert_eq!(s.message, message);
    assert_eq!(s.error_cells, vec![80]);
}

#[test]
fn a_timeout_keeps_useful_advice_but_a_new_puzzle_or_replacement_hint_does_not() {
    let mut s = session();
    hint(&mut s, true);
    let message = s.message.clone();
    let targets = s.targets().to_vec();
    let focus = s.focus.clone();
    s.edit(99, Some(Mark::Cross));
    s.receive(s.revision, Advice::Limit, false);
    assert_eq!(s.hint_text(), Some(message.as_str()));
    assert_ne!(s.message, message);
    assert_eq!(s.targets(), targets);
    assert_eq!(s.focus, focus);
    s.reset();
    assert!(!s.has_hint());
    assert_ne!(s.message, message);
    assert!(s.targets().is_empty());
    assert!(s.focus.is_empty());
    hint(&mut s, false);
    s.edit(88, Some(Mark::Star));
    hint(&mut s, false);
    assert_ne!(s.message, message);
}

#[test]
fn transient_status_messages_do_not_replace_the_stored_hint() {
    let mut s = session();
    hint(&mut s, false);
    let text = s.hint_text().unwrap().to_owned();
    s.message = "No compatible image in the clipboard.".into();
    s.message_error = true;
    assert_eq!(s.hint_text(), Some(text.as_str()));
    s.changed();
    assert_eq!(s.message, text);
    assert!(!s.message_error);
}

#[test]
fn a_timed_out_explanation_keeps_the_hint_but_reports_why_it_did_not_open() {
    let mut s = session();
    hint(&mut s, false);
    let clue = s.hint_text().unwrap().to_owned();
    let focus = s.focus.clone();
    s.changed(); // A drag cancelled without a move discards old tutorial snapshots.
    s.receive(s.revision, Advice::Limit, false);
    assert_eq!(s.hint_text(), Some(clue.as_str()));
    assert_eq!(s.focus, focus);
    assert!(s.can_explain());
    assert!(!s.can_request_hint());
    assert_ne!(
        s.message, clue,
        "The timeout must not be hidden by the retained clue"
    );
}
