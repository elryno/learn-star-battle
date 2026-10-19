//! Native test automation; excluded from ordinary builds.
use super::*;

impl Desktop {
    fn teaching_capture(self: &Rc<Self>, path: PathBuf, mode: String) {
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_enable_animations(false);
        }
        if mode.ends_with("compact") {
            self.window.set_default_size(740, 540);
        }
        if mode.ends_with("narrow") {
            self.window.set_default_size(740, 860);
        }
        if mode.ends_with("dark") {
            adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        }
        let original: Board =
            serde_json::from_str(include_str!("../../tests/fixtures/10x10-daily-page.json"))
                .unwrap();
        self.state.borrow_mut().session.game.load(original.clone());
        self.state.borrow_mut().session.reset();
        self.persist();
        self.hint.emit_clicked();
        let weak = Rc::downgrade(self);
        let mut phase = 0;
        let mut fixed_bounds = None;
        let mut navigation_bounds = None;
        let mut action_bounds = None;
        glib::timeout_add_local(Duration::from_millis(250), move || {
            let Some(ui) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let state = ui.state.borrow();
            if state.busy {
                return glib::ControlFlow::Continue;
            }
            assert_eq!(ui.window.title().as_deref(), Some("Learn Star Battle"));
            assert_eq!(ui.hint.is_sensitive(), !state.session.has_hint());
            let bounds = [
                &ui.board.clone().upcast::<gtk::Widget>(),
                ui.hint.upcast_ref(),
            ]
            .map(|w| {
                let b = w.compute_bounds(&ui.window).unwrap();
                (b.x(), b.y(), b.width(), b.height())
            });
            if let Some(expected) = fixed_bounds {
                assert_eq!(bounds, expected, "Tutorial shifted board or Hint");
            } else {
                fixed_bounds = Some(bounds);
            }
            if phase != 10 && phase != 14 {
                assert_eq!(
                    state.board(),
                    Some(&original),
                    "Lesson changed the real puzzle"
                );
                assert_eq!(
                    storage::load(&ui.save_dir).unwrap().board.as_ref(),
                    Some(&original)
                );
            }
            let hypothetical = state.session.lesson_frame().is_some();
            if state.session.lesson_index.is_some() {
                let b = ui.proof_next.compute_bounds(&ui.window).unwrap();
                let actual = (b.x(), b.y(), b.width(), b.height());
                if let Some(expected) = navigation_bounds {
                    assert_eq!(actual, expected, "Next moved between lesson pages");
                } else {
                    navigation_bounds = Some(actual);
                }
            }
            if hypothetical {
                assert!(!ui.apply_hint.is_visible());
                assert!(ui.back_to_puzzle.is_visible());
                assert!(!ui.undo.is_sensitive());
                assert!(!ui.window.lookup_action("undo").unwrap().is_enabled());
                assert!(ui.status_heading.text().contains("Trial"));
            }
            for button in [&ui.explain, &ui.back_to_puzzle, &ui.apply_hint] {
                if button.is_visible() {
                    let b = button.compute_bounds(&ui.window).unwrap();
                    let actual = (b.x(), b.y(), b.width(), b.height());
                    if let Some(expected) = action_bounds {
                        assert_eq!(actual, expected, "Advice action moved between pages");
                    } else {
                        action_bounds = Some(actual);
                    }
                }
            }
            let scroll = ui.advice_scroll.vadjustment();
            assert!(
                scroll.upper() <= scroll.page_size(),
                "Tutorial needs scrolling at phase {phase}"
            );
            match phase {
                0 | 7 | 12 => assert!(ui.explain.is_visible()),
                1 | 8 => assert_eq!(state.session.lesson_frame().unwrap().marks[62], Mark::Star),
                4 | 6 => assert!(!state.session.lesson_frame().unwrap().conflicts.is_empty()),
                5 | 9 | 13 => {
                    assert!(!hypothetical);
                    assert!(ui.apply_hint.is_visible());
                    assert_eq!(state.session.targets(), &[(62, Mark::Cross)]);
                }
                10 | 14 => {
                    assert!(!state.session.has_hint());
                    assert!(ui.message.text().is_empty());
                    assert!(!ui.apply_hint.is_visible());
                    assert_eq!(state.board().unwrap().marks[62], Mark::Cross);
                }
                _ => {}
            }
            let retained = state.session.has_hint();
            let lesson_index = state.session.lesson_index;
            let message = state.session.message.clone();
            drop(state);
            if retained {
                // Signal emission bypasses widget sensitivity: the callback
                // must also reject a duplicate request without restarting advice.
                ui.hint.emit_clicked();
                let state = ui.state.borrow();
                assert!(!state.busy);
                assert_eq!(state.session.lesson_index, lesson_index);
                assert_eq!(state.session.message, message);
            }
            let snapshot = gtk::Snapshot::new();
            gtk::WidgetPaintable::new(Some(&ui.window)).snapshot(
                &snapshot,
                ui.window.width() as f64,
                ui.window.height() as f64,
            );
            let Some(node) = snapshot.to_node() else {
                return glib::ControlFlow::Continue;
            };
            ui.window
                .renderer()
                .unwrap()
                .render_texture(&node, None)
                .save_to_png(path.with_extension(format!("{phase}.png")))
                .unwrap();
            if hypothetical {
                let g = Geometry::new(ui.board.width(), ui.board.height(), 10);
                let (x, y) = g.origin(62);
                assert!(!ui.begin_board_drag(x + g.cell / 2., y + g.cell / 2.));
                ui.edit(62, Some(Mark::Star));
                assert_eq!(ui.state.borrow().board(), Some(&original));
            }
            match phase {
                0 | 7 => ui.explain.emit_clicked(),
                1..=4 => ui.proof_next.emit_clicked(),
                5 => ui.proof_previous.emit_clicked(),
                6 => ui.back_to_puzzle.emit_clicked(),
                8 => {
                    for _ in 0..4 {
                        ui.proof_next.emit_clicked();
                    }
                }
                9 => {
                    let g = Geometry::new(ui.board.width(), ui.board.height(), 10);
                    let (x, y) = g.origin(62);
                    assert!(ui.begin_board_drag(x + g.cell / 2., y + g.cell / 2.));
                    ui.finish_board_drag(0., 0.);
                }
                10 => ui.undo.emit_clicked(),
                11 => ui.hint.emit_clicked(),
                12 => {
                    ui.explain.emit_clicked();
                    for _ in 0..4 {
                        ui.proof_next.emit_clicked();
                    }
                }
                13 => ui.apply_hint.emit_clicked(),
                14 => {
                    ui.undo.emit_clicked();
                    assert_eq!(ui.state.borrow().board(), Some(&original));
                    assert!(!ui.state.borrow().session.game.can_undo());
                    eprintln!("Native GTK validation passed: {mode}");
                    ui.window.close();
                    return glib::ControlFlow::Break;
                }
                _ => unreachable!(),
            }
            phase += 1;
            glib::ControlFlow::Continue
        });
    }

    // Opt-in native-widget validation. All saves go to the caller's isolated
    // XDG_DATA_HOME; ordinary builds contain none of these automation hooks.
    pub(super) fn validation_capture(self: &Rc<Self>) {
        let Some(path) = std::env::var_os("STAR_BATTLE_CAPTURE_PATH") else {
            return;
        };
        let mode =
            std::env::var("STAR_BATTLE_CAPTURE_SCENARIO").unwrap_or_else(|_| "import".into());
        if mode == "clipboard" {
            let weak = Rc::downgrade(self);
            let mut started = false;
            glib::timeout_add_local(Duration::from_millis(250), move || {
                let Some(ui) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if !started {
                    if !ui.window.is_active() {
                        return glib::ControlFlow::Continue;
                    }
                    eprintln!(
                        "Clipboard receiver: {}",
                        gtk::prelude::WidgetExt::display(&ui.window).type_().name()
                    );
                    ui.paste.emit_clicked();
                    started = true;
                    return glib::ControlFlow::Continue;
                }
                let state = ui.state.borrow();
                if state.busy {
                    return glib::ControlFlow::Continue;
                }
                let review = state
                    .review
                    .as_ref()
                    .unwrap_or_else(|| panic!("Clipboard failed: {}", ui.message.text()));
                let expected: Board = serde_json::from_str(include_str!(
                    "../../tests/fixtures/10x10-daily-page.json"
                ))
                .unwrap();
                assert_eq!(review.board, expected);
                let snapshot = gtk::Snapshot::new();
                gtk::WidgetPaintable::new(Some(&ui.window)).snapshot(
                    &snapshot,
                    ui.window.width() as f64,
                    ui.window.height() as f64,
                );
                let node = snapshot.to_node().unwrap();
                ui.window
                    .renderer()
                    .unwrap()
                    .render_texture(&node, None)
                    .save_to_png(PathBuf::from(&path))
                    .unwrap();
                eprintln!("PASS native desktop clipboard: all regions, colors and marks match");
                drop(state);
                ui.window.close();
                glib::ControlFlow::Break
            });
            return;
        }
        if mode == "external" {
            // This mode is driven through real desktop mouse/keyboard events.
            let weak = Rc::downgrade(self);
            let path = PathBuf::from(path).with_extension("json");
            glib::timeout_add_local(Duration::from_millis(100), move || {
                let Some(ui) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let state = ui.state.borrow();
                let mut widgets = serde_json::Map::new();
                for (name, widget) in [
                    ("paste", ui.paste.upcast_ref::<gtk::Widget>()),
                    ("accept", ui.accept.upcast_ref()),
                    ("cancel", ui.cancel.upcast_ref()),
                    ("source", ui.source_toggle.upcast_ref()),
                    ("regions", ui.region_toggle.upcast_ref()),
                    ("region", ui.region.upcast_ref()),
                    ("hint", ui.hint.upcast_ref()),
                    ("explain", ui.explain.upcast_ref()),
                    ("apply", ui.apply_hint.upcast_ref()),
                    ("previous", ui.proof_previous.upcast_ref()),
                    ("next", ui.proof_next.upcast_ref()),
                    ("back", ui.back_to_puzzle.upcast_ref()),
                    ("undo", ui.undo.upcast_ref()),
                    ("redo", ui.redo.upcast_ref()),
                    ("new", ui.new_puzzle.upcast_ref()),
                    ("help", ui.controls.upcast_ref()),
                    ("menu", ui.menu.upcast_ref()),
                ] {
                    if let Some(b) = widget.compute_bounds(&ui.window) {
                        widgets.insert(
                            name.into(),
                            serde_json::json!({
                                "x": b.x() + b.width()/2., "y": b.y() + b.height()/2.,
                                "width": b.width(), "height": b.height(),
                                "allocated": widget.width() > 0 && widget.height() > 0,
                                "visible": widget.is_mapped(), "enabled": widget.is_sensitive()
                            }),
                        );
                    }
                }
                let grid = state.board().map(|b| {
                    let g = Geometry::new(ui.board.width(), ui.board.height(), b.size);
                    let bounds = ui.board.compute_bounds(&ui.window).unwrap();
                    serde_json::json!({"x": bounds.x() as f64 + g.left, "y": bounds.y() as f64 + g.top, "cell": g.cell})
                });
                let observation = serde_json::json!({
                    "widgets": widgets, "grid": grid, "board": state.board(),
                    "busy": state.busy, "review": state.review.is_some(),
                    "source": ui.source_toggle.is_active(),
                    "text": ui.message.text().as_str(), "heading": ui.status_heading.text().as_str(),
                    "lesson": state.session.lesson_index,
                    "auto": state.session.game.auto_crosses, "errors": state.session.game.show_errors,
                    "help_open": ui.controls.popover().unwrap().is_visible(),
                    "menu_open": ui.menu.popover().unwrap().is_visible(),
                });
                let temporary = path.with_extension("tmp");
                std::fs::write(&temporary, serde_json::to_vec(&observation).unwrap()).unwrap();
                std::fs::rename(temporary, &path).unwrap();
                glib::ControlFlow::Continue
            });
            return;
        }
        if mode.starts_with("teaching") {
            self.teaching_capture(PathBuf::from(path), mode);
            return;
        }
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_enable_animations(false);
        }
        let layout = matches!(mode.as_str(), "layout" | "compact" | "narrow");
        let reasoning = matches!(mode.as_str(), "reasoning" | "reasoning-narrow");
        let source_alignment = mode.starts_with("source-align");
        if matches!(mode.as_str(), "compact" | "source-align-compact") {
            self.window.set_default_size(740, 540);
        } else if matches!(
            mode.as_str(),
            "narrow" | "reasoning-narrow" | "source-align-narrow"
        ) {
            self.window.set_default_size(740, 860);
        }
        if std::env::var_os("STAR_BATTLE_CAPTURE_DARK").is_some() {
            adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        }
        if let Some(file) = std::env::var_os("STAR_BATTLE_TEST_BOARD") {
            let board: Board = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
            let mut state = self.state.borrow_mut();
            state.session.game.load(board);
            state.session.changed();
        }
        match mode.as_str() {
            "help" | "about" => self.window.lookup_action(&mode).unwrap().activate(None),
            "hint" | "timeout" => self.hint.emit_clicked(),
            "solved" => {
                self.auto_crosses.change_state(&false.to_variant());
                assert!(!self.solved_new_puzzle.is_visible());
                self.edit(88, Some(Mark::Star));
            }
            "preview" | "retained-hint" | "counting" | "reasoning" | "reasoning-narrow" => {
                self.request_advice(true)
            }
            "correction" | "correction-preview" | "retained-correction" => {
                self.edit(80, Some(Mark::Star));
                if mode != "correction-preview" {
                    self.hint.emit_clicked();
                } else {
                    self.request_advice(true);
                }
            }
            "errors" => {
                self.edit(80, Some(Mark::Star));
                self.edit(88, Some(Mark::Star));
                let row_error = self
                    .state
                    .borrow()
                    .board()
                    .unwrap()
                    .violations()
                    .iter()
                    .position(|v| v.message.starts_with("Row I has"))
                    .unwrap();
                self.conflicts
                    .select_row(self.conflicts.row_at_index(row_error as i32).as_ref());
            }
            "error-toggle" => {
                assert!(self.state.borrow().session.game.show_errors);
                self.edit(80, Some(Mark::Star));
            }
            "auto" | "drag" => {
                let board = Board::new(
                    10,
                    (0..100).map(|i| i / 10).collect(),
                    vec![Mark::Empty; 100],
                )
                .unwrap();
                {
                    let mut state = self.state.borrow_mut();
                    state.session.game.load(board);
                    state.session.changed();
                }
                self.refresh();
                self.auto_crosses.change_state(&true.to_variant());
                self.edit(33, Some(Mark::Cross));
                self.edit(44, Some(Mark::Star));
            }
            _ => {}
        }
        self.refresh();
        let weak = Rc::downgrade(self);
        let paintable = gtk::WidgetPaintable::new(Some(&self.window));
        let mut settled = 0;
        let mut layout_bounds = None;
        let mut comparison_bounds = None;
        glib::timeout_add_local(Duration::from_millis(200), move || {
            let Some(ui) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let state = ui.state.borrow();
            if state.busy || (mode == "import" && state.review.is_none()) {
                return glib::ControlFlow::Continue;
            }
            settled += 1;
            if settled < 3 {
                return glib::ControlFlow::Continue;
            }
            if mode == "timeout" && settled == 3 {
                let clue = state.session.hint_text().unwrap().to_owned();
                let focus = state.session.focus.clone();
                drop(state);
                let mut state = ui.state.borrow_mut();
                state.session.changed();
                let (tx, receiver) = mpsc::channel();
                state.pending = Some(Pending {
                    revision: state.session.revision,
                    receiver,
                });
                state.busy = true;
                tx.send(ResultMessage::Advised(Advice::Limit, true))
                    .unwrap();
                drop(state);
                ui.poll();
                assert_eq!(ui.state.borrow().session.focus, focus);
                assert!(ui.message.text().contains(&clue));
                assert!(ui.message.text().contains("could not be completed in time"));
                assert!(!ui.hint.is_sensitive());
                assert!(ui.explain.is_visible() && ui.explain.is_sensitive());
                assert_eq!(ui.status_heading.text(), "Hint");
                return glib::ControlFlow::Continue;
            }
            if source_alignment {
                let review = state
                    .review
                    .as_ref()
                    .expect("screenshot ready for comparison");
                assert_eq!(ui.source_toggle.is_active(), settled == 4);
                assert!(ui.board.is_mapped());
                let bounds = [
                    ui.board.upcast_ref::<gtk::Widget>(),
                    ui.review_controls.upcast_ref(),
                    ui.status_frame.upcast_ref(),
                ]
                .map(|widget| {
                    let rect = widget.compute_bounds(&ui.window).unwrap();
                    (rect.x(), rect.y(), rect.width(), rect.height())
                });
                if let Some(expected) = comparison_bounds {
                    assert_eq!(
                        bounds, expected,
                        "Toggling source moved the board or controls"
                    );
                } else {
                    comparison_bounds = Some(bounds);
                }
                let g = Geometry::new(ui.board.width(), ui.board.height(), review.board.size);
                assert!(g.left + g.cell * g.size as f64 <= ui.board.width() as f64);
                assert!(g.top + g.cell * g.size as f64 <= ui.board.height() as f64);
            }
            if layout || reasoning {
                assert!(ui.board.is_mapped());
                assert!(ui.status_frame.is_mapped());
                let bounds = [
                    ui.board.upcast_ref::<gtk::Widget>(),
                    ui.hint.upcast_ref(),
                    ui.undo.upcast_ref(),
                    ui.redo.upcast_ref(),
                    ui.controls.upcast_ref(),
                    ui.menu.upcast_ref(),
                    ui.new_puzzle.upcast_ref(),
                ]
                .map(|widget| {
                    let rect = widget.compute_bounds(&ui.window).unwrap();
                    (rect.x(), rect.y(), rect.width(), rect.height())
                });
                if let Some(expected) = layout_bounds {
                    assert_eq!(bounds, expected, "Controls moved in layout phase {settled}");
                } else {
                    layout_bounds = Some(bounds);
                }
                for widget in [
                    &ui.board.clone().upcast::<gtk::Widget>(),
                    ui.status_frame.upcast_ref(),
                ] {
                    let rect = widget.compute_bounds(&ui.window).unwrap();
                    assert!(
                        rect.y() >= 0. && rect.y() + rect.height() <= ui.window.height() as f32
                    );
                }
                let advice = ui.advice_scroll.vadjustment();
                assert!(
                    advice.upper() <= advice.page_size(),
                    "Advice needs scrolling in {mode} phase {settled}: {} > {}",
                    advice.upper(),
                    advice.page_size()
                );
                if mode == "compact" {
                    assert!(ui.window.height() <= 540);
                    let next_bounds = ui.hint.compute_bounds(&ui.window).unwrap();
                    assert!(next_bounds.y() + next_bounds.height() <= ui.window.height() as f32);
                }
            }
            if layout {
                match settled {
                    4 => {
                        assert!(state.session.advice.is_some());
                    }
                    5 => {
                        assert!(state.session.preview);
                    }
                    6 => {
                        assert!(state.session.game.can_undo());
                        assert!(!state.session.has_hint());
                        assert!(state.session.targets().is_empty());
                        assert!(ui.message.text().is_empty());
                        assert!(ui.status_heading.text().is_empty());
                        assert!(!ui.proof.is_visible());
                        assert!(!ui.apply_hint.is_visible());
                    }
                    7 => assert!(state.session.game.can_redo()),
                    8 => assert!(state.session.game.can_undo()),
                    9 => assert!(!state.session.game.auto_crosses),
                    10 => assert!(ui.controls.popover().unwrap().is_visible()),
                    11 => assert!(!ui.controls.popover().unwrap().is_visible()),
                    12 => assert!(ui.menu.popover().unwrap().is_visible()),
                    13 => assert!(!ui.menu.popover().unwrap().is_visible()),
                    _ => {}
                }
            }
            if mode == "new-puzzle" && settled == 3 {
                let before = state.board().unwrap().clone();
                drop(state);
                assert!(!ui.paste.is_sensitive());
                assert!(!ui.paste.is_mapped());
                let paste_action = ui.window.lookup_action("paste").unwrap();
                assert!(!paste_action.is_enabled());
                paste_action.activate(None);
                ui.paste.emit_clicked();
                assert!(!ui.state.borrow().busy);
                assert_eq!(ui.state.borrow().board(), Some(&before));
                ui.auto_crosses.change_state(&false.to_variant());
                let before = ui.state.borrow().board().unwrap().clone();
                for response in ["cancel", "new"] {
                    ui.new_puzzle.emit_clicked();
                    let dialog = gtk::Window::list_toplevels()
                        .into_iter()
                        .filter_map(|window| window.downcast::<adw::MessageDialog>().ok())
                        .find(|dialog| dialog.is_visible())
                        .expect("New puzzle confirmation");
                    dialog.response(response);
                    if response == "cancel" {
                        assert_eq!(ui.state.borrow().board(), Some(&before));
                        assert!(!ui.paste.is_sensitive());
                    }
                }
                assert!(ui.state.borrow().board().is_none());
                assert!(!ui.auto_crosses.state().unwrap().get::<bool>().unwrap());
                assert!(ui.paste.is_sensitive());
                assert!(paste_action.is_enabled());
                assert!(!ui.new_puzzle.is_sensitive());
                assert!(!ui.undo.is_sensitive());
                assert!(!ui.redo.is_sensitive());
                let saved = storage::load(&ui.save_dir).unwrap();
                assert!(saved.board.is_none());
                assert!(!saved.auto_crosses);
                ui.import(Some(PathBuf::from("tests/fixtures/5x5-empty.jpg")));
                assert!(!ui.paste.is_sensitive());
                assert!(!paste_action.is_enabled());
                return glib::ControlFlow::Continue;
            }
            if mode == "new-puzzle" && settled >= 5 {
                assert!(ui.paste.is_mapped());
                assert!(!ui.play.is_visible());
                assert!(!ui.status_frame.is_visible());
            }
            if mode == "new-puzzle" && settled == 4 {
                assert!(state.review.is_some());
                let before = state.board().unwrap().clone();
                drop(state);
                assert!(!ui.paste.is_sensitive());
                assert!(!ui.window.lookup_action("paste").unwrap().is_enabled());
                ui.import(None);
                assert!(!ui.state.borrow().busy);
                assert_eq!(ui.state.borrow().board(), Some(&before));
                ui.cancel.emit_clicked();
                assert!(ui.state.borrow().board().is_none());
                assert!(ui.paste.is_sensitive());
                assert!(ui.window.lookup_action("paste").unwrap().is_enabled());
                return glib::ControlFlow::Continue;
            }
            if mode.starts_with("retained-") && settled == 3 {
                let message = state.session.message.clone();
                let focus = state.session.focus.clone();
                drop(state);
                if mode == "retained-hint" {
                    // Follow only half of the hint through the mouse click path.
                    let g = Geometry::new(ui.board.width(), ui.board.height(), 10);
                    let (x, y) = g.origin(80);
                    assert!(ui.begin_board_drag(x + g.cell / 2., y + g.cell / 2.));
                    ui.finish_board_drag(0., 0.);
                    assert_eq!(ui.state.borrow().session.focus, focus);
                } else {
                    ui.conflicts
                        .select_row(ui.conflicts.row_at_index(0).as_ref());
                    assert!(ui.state.borrow().session.focus.is_empty());
                }
                assert!(ui.state.borrow().session.has_hint());
                assert_eq!(ui.message.text(), message);
                assert!(ui.state.borrow().session.error_cells.is_empty());
                return glib::ControlFlow::Continue;
            }
            if mode == "drag" && settled == 3 {
                drop(state);
                let g = Geometry::new(ui.board.width(), ui.board.height(), 10);
                let (x, y) = g.origin(44);
                assert!(ui.begin_board_drag(x + g.cell / 2., y + g.cell / 2.));
                ui.update_board_drag(-4. * g.cell, 0.);
                ui.update_board_drag(5. * g.cell, 0.);
                assert_eq!(ui.state.borrow().board().unwrap().marks[40], Mark::Empty);
                ui.finish_board_drag(5. * g.cell, 0.);
                return glib::ControlFlow::Continue;
            }
            if mode.starts_with("retained-") {
                assert_eq!(
                    ui.status_heading.text(),
                    if mode == "retained-hint" {
                        "Conclusion"
                    } else {
                        "Hint"
                    }
                );
                assert!(state.session.has_hint());
                assert!(!state.session.preview);
                assert!(state.session.advice.is_none());
                if mode == "retained-hint" {
                    assert_eq!(state.session.targets(), &[(90, Mark::Cross)]);
                    assert!(state.session.has_proposed_marks());
                    assert_eq!(state.board().unwrap().marks[80], Mark::Cross);
                }
            } else if mode == "counting" {
                assert!(state.session.preview);
                assert_eq!(
                    state.session.targets(),
                    &[(60, Mark::Cross), (62, Mark::Cross), (63, Mark::Cross)]
                );
                assert!(state.session.advice.as_ref().unwrap().frames.is_empty());
                assert!(!ui.proof.is_visible());
                assert!(ui.message.text().contains("Rows H–J need 6 stars"));
                assert!(ui.message.text().contains("G1, G3, G4"));
                assert_eq!(state.board().unwrap().marks[38], Mark::Empty);
            } else if mode == "preview" {
                assert!(state.session.preview);
                assert_eq!(
                    state.session.game.board.as_ref().unwrap().marks[80],
                    Mark::Empty
                );
                assert_eq!(
                    state.session.targets(),
                    &[(80, Mark::Cross), (90, Mark::Cross)]
                );
            } else if mode == "solved" && settled == 3 {
                assert!(state.board().unwrap().solved());
                assert_eq!(ui.status_heading.text(), "Puzzle Solved");
                assert!(ui.solved_new_puzzle.is_mapped());
                let footer = ui.lesson_controls.compute_bounds(&ui.window).unwrap();
                let action = ui.solved_new_puzzle.compute_bounds(&ui.window).unwrap();
                let panel = ui.status_frame.compute_bounds(&ui.window).unwrap();
                assert_eq!(action.y(), footer.y());
                assert!(
                    (panel.y() + panel.height() - action.y() - action.height() - 15.).abs() < 2.
                );
                assert!(!ui.hint.is_sensitive());
                assert!(!ui.apply_hint.is_sensitive());
                assert!(!ui.paste.is_sensitive());
            } else if mode == "solved" && settled == 4 {
                assert!(state.board().is_none());
                assert!(ui.paste.is_mapped());
                assert!(!ui.solved_new_puzzle.is_visible());
            } else if mode == "error-toggle" {
                assert!(!state.board().unwrap().violations().is_empty());
                match settled {
                    3 => {
                        assert!(ui.conflicts.is_visible());
                        assert_eq!(ui.status_heading.text(), "Conflicts");
                        assert!(
                            !state
                                .session
                                .conflict_cells(state.board().unwrap())
                                .is_empty()
                        );
                    }
                    4 => {
                        assert!(!ui.conflicts.is_visible());
                        assert!(ui.message.text().is_empty());
                        assert!(ui.status_heading.text().is_empty());
                        assert!(
                            state
                                .session
                                .conflict_cells(state.board().unwrap())
                                .is_empty()
                        );
                    }
                    5 => {
                        assert!(!ui.conflicts.is_visible());
                        assert_eq!(state.session.error_cells, vec![80]);
                        assert!(ui.message.text().contains("Change the star at I1 to an X"));
                    }
                    _ => unreachable!(),
                }
            } else if mode == "errors" {
                assert_eq!(
                    state.session.conflict_cells(state.board().unwrap()),
                    (80..90).collect::<Vec<_>>()
                );
            } else if mode == "auto" {
                assert!(ui.auto_crosses.state().unwrap().get::<bool>().unwrap());
                assert_eq!(state.board().unwrap().automatic_crosses.len(), 7);
                assert!(!state.board().unwrap().automatic_crosses.contains(&33));
            } else if mode.starts_with("correction") {
                assert_eq!(state.session.error_cells, vec![80]);
                assert_eq!(state.board().unwrap().marks[80], Mark::Star);
                assert!(ui.message.text().contains("Change the star at I1 to an X"));
                assert!(ui.message.has_css_class("error"));
                assert!(ui.conflicts.selected_row().is_none());
            } else if mode == "drag" {
                assert_eq!(state.board().unwrap().marks[44], Mark::Star);
                for i in (40..50).filter(|&i| i != 44) {
                    assert_eq!(state.board().unwrap().marks[i], Mark::Cross);
                }
            }
            drop(state);
            let snapshot = gtk::Snapshot::new();
            paintable.snapshot(
                &snapshot,
                ui.window.width() as f64,
                ui.window.height() as f64,
            );
            let Some(node) = snapshot.to_node() else {
                ui.window.queue_draw();
                return glib::ControlFlow::Continue;
            };
            let texture = ui.window.renderer().unwrap().render_texture(&node, None);
            let output = if layout
                || reasoning
                || source_alignment
                || matches!(mode.as_str(), "solved" | "error-toggle" | "timeout")
            {
                PathBuf::from(&path).with_extension(format!("{settled}.png"))
            } else {
                PathBuf::from(&path)
            };
            texture
                .save_to_png(&output)
                .expect("save validation screenshot");
            // Popovers use their own native surface, so capture them
            // separately from the application window for visual inspection.
            if layout && matches!(settled, 10 | 12) {
                let (button, name) = if settled == 10 {
                    (&ui.controls, "controls")
                } else {
                    (&ui.menu, "menu")
                };
                let popover = button.popover().unwrap();
                let help_snapshot = gtk::Snapshot::new();
                gtk::WidgetPaintable::new(Some(&popover)).snapshot(
                    &help_snapshot,
                    popover.width() as f64,
                    popover.height() as f64,
                );
                let Some(help_node) = help_snapshot.to_node() else {
                    // A native popup may be visible before its first frame.
                    // Wait for that frame without advancing the scenario.
                    settled -= 1;
                    return glib::ControlFlow::Continue;
                };
                ui.window
                    .renderer()
                    .unwrap()
                    .render_texture(&help_node, None)
                    .save_to_png(PathBuf::from(&path).with_extension(format!("{name}.png")))
                    .expect("save popover screenshot");
            }
            if matches!(mode.as_str(), "help" | "about") {
                let dialog = gtk::Window::list_toplevels()
                    .into_iter()
                    .filter_map(|window| window.downcast::<gtk::Window>().ok())
                    .find(|window| {
                        window.is_visible() && window != ui.window.upcast_ref::<gtk::Window>()
                    })
                    .expect("menu action opened its dialog");
                let dialog_snapshot = gtk::Snapshot::new();
                gtk::WidgetPaintable::new(Some(&dialog)).snapshot(
                    &dialog_snapshot,
                    dialog.width() as f64,
                    dialog.height() as f64,
                );
                let node = dialog_snapshot.to_node().expect("render menu dialog");
                dialog
                    .renderer()
                    .unwrap()
                    .render_texture(&node, None)
                    .save_to_png(&path)
                    .expect("save menu dialog screenshot");
                dialog.close();
            }
            if source_alignment {
                match settled {
                    3 => {
                        ui.source_toggle.set_active(true);
                        let before = ui.state.borrow().board().unwrap().clone();
                        let g = Geometry::new(ui.board.width(), ui.board.height(), before.size);
                        assert!(!ui.begin_board_drag(g.left + g.cell / 2., g.top + g.cell / 2.));
                        assert_eq!(ui.state.borrow().board(), Some(&before));
                    }
                    4 => ui.source_toggle.set_active(false),
                    5 => {}
                    _ => unreachable!(),
                }
                if settled < 5 {
                    return glib::ControlFlow::Continue;
                }
            } else if mode == "timeout" {
                if settled == 4 {
                    ui.explain.emit_clicked();
                    return glib::ControlFlow::Continue;
                }
                assert!(ui.state.borrow().session.can_apply_hint());
                ui.apply_hint.emit_clicked();
                assert!(ui.hint.is_sensitive());
                assert!(ui.message.text().is_empty());
            } else if layout {
                match settled {
                    3 => ui.hint.emit_clicked(),
                    4 => ui.explain.emit_clicked(),
                    5 => ui.apply_hint.emit_clicked(),
                    6 => ui.undo.emit_clicked(),
                    7 => ui.redo.emit_clicked(),
                    8 => ui.auto_crosses.change_state(&false.to_variant()),
                    9 => {
                        ui.controls.grab_focus();
                        ui.controls.popup();
                    }
                    10 => ui.controls.popdown(),
                    11 => {
                        ui.menu.grab_focus();
                        ui.menu.popup();
                    }
                    12 => ui.menu.popdown(),
                    _ => {}
                }
                if settled < 13 {
                    return glib::ControlFlow::Continue;
                }
            } else if mode == "error-toggle" {
                match settled {
                    3 => ui.show_errors.change_state(&false.to_variant()),
                    4 => ui.hint.emit_clicked(),
                    5 => {
                        let before = ui.state.borrow().board().unwrap().clone();
                        let explanation = ui.message.text();
                        ui.show_errors.change_state(&true.to_variant());
                        assert!(ui.conflicts.is_visible());
                        ui.show_errors.change_state(&false.to_variant());
                        assert!(!ui.conflicts.is_visible());
                        assert_eq!(ui.message.text(), explanation);
                        assert!(ui.state.borrow().session.has_hint());
                        assert_eq!(ui.state.borrow().board(), Some(&before));
                        assert!(!storage::load(&ui.save_dir).unwrap().show_errors);
                    }
                    _ => unreachable!(),
                }
                if settled < 5 {
                    return glib::ControlFlow::Continue;
                }
            } else if mode == "solved" && settled == 3 {
                ui.undo.emit_clicked();
                assert!(!ui.solved_new_puzzle.is_visible());
                assert!(ui.hint.is_sensitive());
                ui.redo.emit_clicked();
                assert!(ui.solved_new_puzzle.is_visible());
                ui.solved_new_puzzle.emit_clicked();
                assert!(ui.state.borrow().board().is_none());
                assert!(!ui.undo.is_sensitive());
                assert!(!ui.redo.is_sensitive());
                assert!(ui.paste.is_sensitive());
                assert!(ui.window.lookup_action("paste").unwrap().is_enabled());
                let saved = storage::load(&ui.save_dir).unwrap();
                assert!(saved.board.is_none());
                assert!(!saved.auto_crosses);
                return glib::ControlFlow::Continue;
            } else if mode == "counting" {
                ui.apply_hint.emit_clicked();
                for i in [60, 62, 63] {
                    assert_eq!(ui.state.borrow().board().unwrap().marks[i], Mark::Cross);
                }
                ui.undo.emit_clicked();
                for i in [60, 62, 63] {
                    assert_eq!(ui.state.borrow().board().unwrap().marks[i], Mark::Empty);
                }
            } else if mode.starts_with("retained-") {
                if mode == "retained-hint" {
                    ui.edit(90, Some(Mark::Cross));
                } else {
                    ui.edit(80, Some(Mark::Empty));
                }
                assert!(!ui.state.borrow().session.has_hint());
                assert!(ui.state.borrow().session.focus.is_empty());
                assert_ne!(ui.status_heading.text(), "Hint");
            } else if mode.starts_with("correction") {
                if mode == "correction" {
                    ui.explain.emit_clicked();
                }
                assert!(ui.state.borrow().session.preview);
                assert_eq!(ui.state.borrow().board().unwrap().marks[80], Mark::Star);
                ui.apply_hint.emit_clicked();
                assert_eq!(ui.state.borrow().board().unwrap().marks[80], Mark::Cross);
                ui.undo.emit_clicked();
                assert_eq!(ui.state.borrow().board().unwrap().marks[80], Mark::Star);
            } else if mode == "drag" {
                ui.undo.emit_clicked();
                assert_eq!(ui.state.borrow().board().unwrap().marks[40], Mark::Empty);
                assert_eq!(ui.state.borrow().board().unwrap().marks[44], Mark::Star);
                ui.redo.emit_clicked();
                assert_eq!(ui.state.borrow().board().unwrap().marks[40], Mark::Cross);
                let g = Geometry::new(ui.board.width(), ui.board.height(), 10);
                let (x, y) = g.origin(44);
                assert!(ui.begin_board_drag(x + g.cell / 2., y + g.cell / 2.));
                ui.finish_board_drag(0., 0.); // A click still cycles a star to empty.
                assert_eq!(ui.state.borrow().board().unwrap().marks[44], Mark::Empty);
            } else if mode == "preview" {
                ui.apply_hint.emit_clicked();
                assert_eq!(
                    ui.state.borrow().session.game.board.as_ref().unwrap().marks[80],
                    Mark::Cross
                );
                ui.undo.emit_clicked();
                assert_eq!(
                    ui.state.borrow().session.game.board.as_ref().unwrap().marks[80],
                    Mark::Empty
                );
            } else if mode == "auto" {
                ui.edit(44, None);
                assert!(
                    ui.state
                        .borrow()
                        .board()
                        .unwrap()
                        .automatic_crosses
                        .is_empty()
                );
                assert_eq!(ui.state.borrow().board().unwrap().marks[33], Mark::Cross);
                ui.auto_crosses.change_state(&false.to_variant());
                ui.edit(44, Some(Mark::Star));
                assert_eq!(ui.state.borrow().board().unwrap().marks[34], Mark::Empty);
                ui.auto_crosses.change_state(&true.to_variant());
                assert_eq!(ui.state.borrow().board().unwrap().marks[34], Mark::Cross);
                ui.undo.emit_clicked();
                assert_eq!(ui.state.borrow().board().unwrap().marks[44], Mark::Empty);
                assert_eq!(ui.state.borrow().board().unwrap().marks[34], Mark::Empty);
                ui.redo.emit_clicked();
                assert_eq!(
                    ui.state.borrow().board().unwrap().automatic_crosses.len(),
                    7
                );
                assert_eq!(
                    storage::load(&ui.save_dir).unwrap().board,
                    ui.state.borrow().session.game.board
                );
                ui.auto_crosses.change_state(&false.to_variant());
                assert!(!storage::load(&ui.save_dir).unwrap().auto_crosses);
            }
            eprintln!("Native GTK validation passed: {mode}");
            ui.window.close();
            glib::ControlFlow::Break
        });
    }
}
