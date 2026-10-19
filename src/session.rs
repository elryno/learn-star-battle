use crate::{
    model::{Board, Game, Mark},
    solver::{Advice, ReasoningFrame, Step},
};

/// Text, supporting highlights and revealed targets survive edits while their
/// facts and unfinished actions still apply. This is not an executable cached step.
struct RetainedHint {
    size: usize,
    regions: Vec<usize>,
    premises: Vec<(usize, Mark)>,
    targets: Vec<(usize, Mark, Mark)>,
    revealed: bool,
    message: String,
    base_message: String,
    error: bool,
    focus: Vec<usize>,
}

impl RetainedHint {
    fn relevant(&self, board: &Board) -> bool {
        self.size == board.size
            && self.regions == board.regions
            && !board.solved()
            && self
                .premises
                .iter()
                .all(|&(i, mark)| board.marks[i] == mark)
            && (if self.targets.is_empty() {
                !self.premises.is_empty()
            } else {
                self.targets
                    .iter()
                    .any(|&(i, _, wanted)| board.marks[i] != wanted)
                    && self.targets.iter().all(|&(i, original, wanted)| {
                        board.marks[i] == original || board.marks[i] == wanted
                            // The mouse cycle passes through X on the way to a
                            // suggested star. Keep teaching through that click.
                            || (original == Mark::Empty && wanted == Mark::Star
                                && board.marks[i] == Mark::Cross)
                    })
            })
    }
}

/// State transitions shared by mouse, keyboard and asynchronous analysis.
pub struct Session {
    pub game: Game,
    pub revision: u64,
    pub advice: Option<Step>,
    pub preview: bool,
    proposed_changes: Vec<(usize, Mark)>,
    pub message: String,
    pub message_error: bool,
    pub error_cells: Vec<usize>,
    pub focus: Vec<usize>,
    /// Includes the conclusion at index frames.len(); only earlier pages simulate marks.
    pub lesson_index: Option<usize>,
    pub conflict: usize,
    cross_stroke: Option<Vec<usize>>,
    retained_hint: Option<RetainedHint>,
}

impl Session {
    pub fn new(game: Game) -> Self {
        Self {
            game,
            revision: 0,
            advice: None,
            preview: false,
            proposed_changes: vec![],
            message: String::new(),
            message_error: false,
            error_cells: vec![],
            focus: vec![],
            lesson_index: None,
            conflict: 0,
            cross_stroke: None,
            retained_hint: None,
        }
    }
    pub fn changed(&mut self) {
        self.lesson_index = None;
        self.revision += 1;
        self.advice = None;
        self.preview = false;
        self.proposed_changes.clear();
        self.focus.clear();
        self.conflict = 0;
        self.message_error = false;
        self.error_cells.clear();
        self.cross_stroke = None;
        self.message.clear();
        self.refresh_retained_hint();
    }

    pub fn has_hint(&self) -> bool {
        self.retained_hint.is_some()
    }
    pub fn can_request_hint(&self) -> bool {
        !self.has_hint()
            && self
                .game
                .board
                .as_ref()
                .is_some_and(|board| !board.solved())
    }

    pub fn hint_text(&self) -> Option<&str> {
        self.retained_hint
            .as_ref()
            .map(|hint| hint.message.as_str())
    }

    pub fn reset(&mut self) {
        self.retained_hint = None;
        self.changed();
    }

    pub fn new_puzzle(&mut self) {
        let auto_crosses = self.game.auto_crosses;
        let show_errors = self.game.show_errors;
        self.game = Game::default();
        self.game.set_auto_crosses(auto_crosses);
        self.game.show_errors = show_errors;
        self.reset();
        self.message = "Paste a screenshot to start a new puzzle.".into();
    }

    fn refresh_retained_hint(&mut self) {
        if !self.retained_hint.as_ref().is_some_and(|hint| {
            self.game
                .board
                .as_ref()
                .is_some_and(|board| hint.relevant(board))
        }) {
            self.retained_hint = None;
        }
        if let Some(hint) = &mut self.retained_hint {
            hint.message = hint.base_message.clone();
            self.message = hint.message.clone();
            self.message_error = hint.error;
            self.focus = hint.focus.clone();
            if hint.error {
                self.error_cells = hint.focus.clone();
            }
            if hint.revealed {
                if let Some(board) = &self.game.board {
                    let remaining: Vec<_> = hint
                        .targets
                        .iter()
                        .filter(|&&(i, _, wanted)| board.marks[i] != wanted)
                        .map(|&(i, _, wanted)| (i, wanted))
                        .collect();
                    let mut after = board.clone();
                    after.play_marks(&remaining, self.game.auto_crosses);
                    self.proposed_changes = mark_changes(board, &after);
                }
            }
        }
    }
    pub fn edit(&mut self, cell: usize, mark: Option<Mark>) {
        if self.lesson_frame().is_some() {
            return;
        }
        let Some(board) = &self.game.board else {
            return;
        };
        let mark = mark.unwrap_or_else(|| board.marks[cell].next());
        self.game.play(&[(cell, mark)]);
        self.changed();
    }
    pub fn set_auto_crosses(&mut self, enabled: bool) {
        self.game.set_auto_crosses(enabled);
        self.changed();
        if !self.has_hint() {
            self.message = if enabled {
            "Xs are added around stars and in completed rows, columns, and regions. Removing a star clears any automatic Xs that are no longer needed. Your manual marks stay."
        } else {
            "Automatic Xs are off. Your manual marks stay."
            }.into();
        }
    }
    pub fn receive(&mut self, revision: u64, advice: Advice, preview: bool) {
        if revision != self.revision {
            return;
        }
        self.lesson_index = None;
        self.message_error = false;
        self.error_cells.clear();
        self.advice = None;
        self.preview = false;
        self.proposed_changes.clear();
        self.focus.clear();
        let previous_hint = self.retained_hint.take();
        match advice {
            Advice::Step(step) => { self.advice = Some(step); self.reveal(preview); }
            Advice::Correction(step) => {
                self.error_cells = step.focus.clone();
                self.message_error = true;
                self.advice = Some(step);
                self.reveal(preview);
            }
            Advice::Contradiction { message, cells } => {
                self.message = message;
                if !cells.is_empty() {
                    if let Some(board) = &self.game.board {
                        self.retained_hint = Some(RetainedHint {
                            size: board.size,
                            regions: board.regions.clone(),
                            premises: cells.iter().map(|&i| (i, board.marks[i])).collect(),
                            targets: vec![],
                            revealed: false,
                            message: self.message.clone(),
                            base_message: self.message.clone(),
                            error: true,
                            focus: cells.clone(),
                        });
                    }
                }
                self.error_cells = cells;
                self.message_error = true;
            }
            Advice::Solved => self.message = "Puzzle solved! All stars are in place and none touch.".into(),
            Advice::NoForcedMove => self.message = "Several solutions are possible, and no cell has the same mark in all of them. Check that the imported regions and marks match your screenshot.".into(),
            Advice::Limit => {
                self.retained_hint = previous_hint;
                self.refresh_retained_hint();
                self.message = if self.has_hint() {
                    "The explanation could not be completed in time. Your existing hint is still available."
                } else {
                    "Finding a hint is taking too long. Try making a move yourself, then ask again."
                }.into();
            }
        }
    }
    pub fn reveal(&mut self, preview: bool) {
        if let Some(step) = &self.advice {
            self.preview = preview;
            self.focus = if let Some(first) = step.frames.first() {
                if preview {
                    step.changes.iter().map(|&(i, _)| i).collect()
                } else {
                    first.focus.clone()
                }
            } else {
                step.focus.clone()
            };
            self.message = if preview {
                step.explanation.clone()
            } else {
                step.clue.clone()
            };
            self.proposed_changes.clear();
            if preview {
                if let Some(board) = &self.game.board {
                    let mut after = board.clone();
                    after.play_marks(&step.changes, self.game.auto_crosses);
                    self.proposed_changes = mark_changes(board, &after);
                    let automatic: Vec<_> = self
                        .proposed_changes
                        .iter()
                        .filter(|(i, _)| after.automatic_crosses.contains(i))
                        .map(|&(i, _)| board.address(i))
                        .collect();
                    if !automatic.is_empty() {
                        self.message.push_str(&format!(
                            " Xs will also be added at {}.",
                            automatic.join(", ")
                        ));
                    }
                }
            }
            if let Some(board) = &self.game.board {
                self.retained_hint = Some(RetainedHint {
                    size: board.size,
                    regions: board.regions.clone(),
                    premises: step.premises.clone(),
                    targets: step
                        .changes
                        .iter()
                        .map(|&(i, wanted)| (i, board.marks[i], wanted))
                        .collect(),
                    revealed: preview,
                    // Automatic-X additions are recalculated after edits;
                    // keep the actual explanation separately from that suffix.
                    message: self.message.clone(),
                    base_message: if preview {
                        step.explanation.clone()
                    } else {
                        step.clue.clone()
                    },
                    error: self.message_error,
                    focus: self.focus.clone(),
                });
            }
        }
    }
    pub fn can_explain(&self) -> bool {
        self.advice.is_some()
            || self
                .retained_hint
                .as_ref()
                .is_some_and(|h| !h.targets.is_empty())
    }
    pub fn lesson_frame(&self) -> Option<&ReasoningFrame> {
        self.advice.as_ref()?.frames.get(self.lesson_index?)
    }
    pub fn lesson_length(&self) -> usize {
        self.advice.as_ref().map_or(0, |s| s.frames.len() + 1)
    }
    pub fn start_explanation(&mut self) {
        let Some(step) = &self.advice else {
            return;
        };
        if step.frames.is_empty() {
            self.lesson_index = None;
            self.reveal(true);
        } else {
            self.reveal(false);
            self.lesson_index = Some(0);
        }
    }
    pub fn navigate_explanation(&mut self, forward: bool) {
        let (Some(index), Some(step)) = (self.lesson_index, &self.advice) else {
            return;
        };
        let last = step.frames.len();
        let index = if forward {
            (index + 1).min(last)
        } else {
            index.saturating_sub(1)
        };
        self.reveal(index == last);
        self.lesson_index = Some(index);
    }
    pub fn leave_explanation(&mut self) {
        if self.lesson_frame().is_some() {
            self.reveal(false);
        }
        self.lesson_index = None;
    }
    pub fn can_apply_hint(&self) -> bool {
        self.lesson_frame().is_none()
            && self.retained_hint.as_ref().is_some_and(|hint| {
                hint.revealed
                    && !hint.targets.is_empty()
                    && self.game.board.as_ref().is_some_and(|b| hint.relevant(b))
            })
    }
    /// Apply only the remaining proven targets, including after partial manual progress.
    pub fn apply_hint(&mut self) {
        if !self.can_apply_hint() {
            return;
        }
        let hint = self.retained_hint.as_ref().unwrap();
        let board = self.game.board.as_ref().unwrap();
        let changes: Vec<_> = hint
            .targets
            .iter()
            .filter(|&&(i, _, wanted)| board.marks[i] != wanted)
            .map(|&(i, _, wanted)| (i, wanted))
            .collect();
        self.game.play(&changes);
        self.reset();
    }
    pub fn targets(&self) -> &[(usize, Mark)] {
        &self.proposed_changes
    }
    pub fn has_proposed_marks(&self) -> bool {
        !self.proposed_changes.is_empty()
    }
    pub fn select_conflict(&mut self, index: usize) {
        self.changed();
        // Explicitly inspecting a rule violation temporarily takes priority
        // over the retained hint's highlights.
        self.focus.clear();
        self.error_cells.clear();
        self.conflict = index;
    }
    pub fn conflict_cells(&self, board: &Board) -> Vec<usize> {
        if !self.error_cells.is_empty() {
            return self.error_cells.clone();
        }
        if !self.game.show_errors {
            return vec![];
        }
        let conflicts = board.violations();
        conflicts
            .get(self.conflict)
            .or_else(|| conflicts.first())
            .map(|v| v.cells.clone())
            .unwrap_or_default()
    }

    pub fn begin_cross_stroke(&mut self) {
        if self.lesson_frame().is_some() {
            return;
        }
        self.changed();
        self.cross_stroke = Some(vec![]);
    }

    pub fn extend_cross_stroke(&mut self, cells: &[usize]) {
        let (Some(board), Some(stroke)) = (&self.game.board, &mut self.cross_stroke) else {
            return;
        };
        for &i in cells {
            if board.marks.get(i) == Some(&Mark::Empty) && !stroke.contains(&i) {
                stroke.push(i);
            }
        }
    }

    pub fn painted_cells(&self) -> &[usize] {
        self.cross_stroke.as_deref().unwrap_or_default()
    }

    pub fn finish_cross_stroke(&mut self) {
        let Some(stroke) = self.cross_stroke.take() else {
            return;
        };
        let changes: Vec<_> = stroke.into_iter().map(|i| (i, Mark::Cross)).collect();
        self.game.play(&changes);
        self.changed();
    }

    pub fn cancel_cross_stroke(&mut self) {
        if self.cross_stroke.take().is_some() {
            self.changed();
        }
    }
}

fn mark_changes(before: &Board, after: &Board) -> Vec<(usize, Mark)> {
    before
        .marks
        .iter()
        .zip(&after.marks)
        .enumerate()
        .filter(|(_, (before, after))| before != after)
        .map(|(i, (_, &mark))| (i, mark))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> Session {
        let mut game = Game::default();
        game.set_auto_crosses(false);
        game.load(Board::new(5, (0..25).map(|i| i / 5).collect(), vec![Mark::Empty; 25]).unwrap());
        Session::new(game)
    }
    fn step() -> Advice {
        Advice::Step(Step {
            clue: "clue".into(),
            explanation: "reason".into(),
            focus: vec![0],
            changes: vec![(0, Mark::Star), (1, Mark::Cross)],
            premises: vec![],
            frames: vec![],
        })
    }
    #[test]
    fn new_puzzle_clears_history_and_hints_but_keeps_settings() {
        let mut s = session();
        s.game.show_errors = false;
        s.edit(20, Some(Mark::Cross));
        s.edit(22, Some(Mark::Cross));
        s.game.undo();
        assert!(s.game.can_undo());
        assert!(s.game.can_redo());
        let old_revision = s.revision;
        s.receive(old_revision, step(), true);
        assert!(s.has_hint());
        assert!(!s.targets().is_empty());

        s.new_puzzle();
        assert!(!s.game.auto_crosses);
        assert!(!s.game.show_errors);
        assert!(s.game.board.is_none());
        assert!(!s.game.can_undo());
        assert!(!s.game.can_redo());
        assert!(!s.has_hint());
        assert!(s.targets().is_empty());
        assert!(s.focus.is_empty());

        // Neither undo/redo nor an old asynchronous result can revive it.
        s.game.undo();
        s.game.redo();
        s.receive(old_revision, step(), true);
        assert!(s.game.board.is_none());
        assert!(s.advice.is_none());
        assert!(s.targets().is_empty());
    }
    #[test]
    fn hint_and_explanation_never_change_marks_until_applied() {
        let mut s = session();
        let Advice::Step(proposal) = step() else {
            unreachable!()
        };
        s.receive(0, Advice::Step(proposal), false);
        assert_eq!(s.message, "clue");
        assert!(s.targets().is_empty());
        s.apply_hint();
        assert_eq!(s.game.board.as_ref().unwrap().marks, vec![Mark::Empty; 25]);
        s.reveal(true);
        assert_eq!(s.message, "reason");
        assert_eq!(s.targets().len(), 2);
        assert_eq!(s.game.board.as_ref().unwrap().marks, vec![Mark::Empty; 25]);
        s.apply_hint();
        assert_eq!(s.game.board.as_ref().unwrap().marks[0], Mark::Star);
        assert!(!s.has_hint());
        assert!(s.focus.is_empty());
        assert!(s.targets().is_empty());
        assert!(s.message.is_empty());
        assert!(!s.preview);
        assert!(s.advice.is_none());
        s.game.undo();
        s.changed();
        assert_eq!(s.game.board.as_ref().unwrap().marks, vec![Mark::Empty; 25]);
        assert!(s.targets().is_empty());
        assert!(s.message.is_empty());
        s.game.redo();
        s.changed();
        assert_eq!(s.game.board.as_ref().unwrap().marks[0], Mark::Star);
        assert!(s.targets().is_empty());
        assert!(s.message.is_empty());
    }
    #[test]
    fn editing_invalidates_previews_and_late_analysis() {
        let mut s = session();
        s.receive(0, step(), true);
        // Contradict the suggested X. Empty -> X on a suggested star is
        // deliberately retained because it is part of the mouse click cycle.
        s.edit(1, Some(Mark::Star));
        s.receive(0, step(), true);
        s.apply_hint();
        assert!(s.advice.is_none());
        assert!(s.focus.is_empty());
        assert!(s.targets().is_empty());
        assert_eq!(s.game.board.unwrap().marks[1], Mark::Star);
    }
    #[test]
    fn only_the_selected_conflict_is_highlighted() {
        let mut s = session();
        s.edit(0, Some(Mark::Star));
        s.edit(2, Some(Mark::Star));
        let board = s.game.board.as_ref().unwrap();
        assert_eq!(s.conflict_cells(board), (0..5).collect::<Vec<_>>());
        s.conflict = board.violations().len() - 1;
        assert_eq!(
            s.conflict_cells(board),
            board.violations().last().unwrap().cells
        );
    }
    #[test]
    fn hiding_live_errors_keeps_requested_corrections_and_rule_checks() {
        let mut s = session();
        s.edit(0, Some(Mark::Star));
        s.edit(2, Some(Mark::Star));
        assert!(!s.conflict_cells(s.game.board.as_ref().unwrap()).is_empty());
        s.game.show_errors = false;
        let board = s.game.board.as_ref().unwrap();
        assert!(s.conflict_cells(board).is_empty());
        assert!(!board.violations().is_empty());
        assert!(!board.solved());
        s.receive(
            s.revision,
            Advice::Contradiction {
                message: "Row A has too many stars; remove one.".into(),
                cells: vec![0, 2],
            },
            false,
        );
        assert_eq!(s.conflict_cells(s.game.board.as_ref().unwrap()), vec![0, 2]);
        assert!(s.message_error);
        s.game.undo();
        s.changed();
        assert!(!s.game.show_errors);
        assert!(s.game.board.as_ref().unwrap().violations().is_empty());
        s.game.redo();
        s.changed();
        assert!(!s.game.show_errors);
        assert!(s.conflict_cells(s.game.board.as_ref().unwrap()).is_empty());
        s.game.show_errors = true;
        assert!(!s.conflict_cells(s.game.board.as_ref().unwrap()).is_empty());
    }
}
