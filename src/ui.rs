#[cfg(feature = "capture")]
mod capture;

use adw::prelude::*;
use gtk::{cairo, gio, glib};
use star_battle_solver::{
    model::{Board, Mark},
    recognition::{Recognized, recognize},
    session::Session,
    solver::{Advice, advise},
    storage,
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

// Fallback for older saves, which did not retain screenshot colors.
const COLORS: [[u8; 3]; 14] = [
    [227, 207, 247],
    [247, 201, 201],
    [192, 237, 207],
    [195, 222, 248],
    [251, 217, 182],
    [188, 235, 235],
    [250, 235, 158],
    [245, 209, 233],
    [212, 233, 190],
    [214, 214, 245],
    [245, 202, 188],
    [198, 236, 223],
    [238, 211, 238],
    [239, 239, 190],
];
const INK: (f64, f64, f64) = (0.12, 0.20, 0.20);
const TEAL: (f64, f64, f64) = (0.0, 0.42, 0.36);
const ORANGE: (f64, f64, f64) = (1., 121. / 255., 0.);
const RED: (f64, f64, f64) = (0.75, 0.12, 0.15);
const MOUSE_CONTROLS: [(&str, &str); 2] = [
    ("Click", "Cycle empty → X → star → empty"),
    ("Drag", "Mark empty cells X; keep existing marks"),
];

struct Review {
    board: Board,
    warnings: Vec<String>,
    source: gtk::gdk_pixbuf::Pixbuf,
}
enum ResultMessage {
    Imported(Result<(Recognized, image::RgbImage), String>),
    Advised(Advice, bool),
}
struct Pending {
    revision: u64,
    receiver: mpsc::Receiver<ResultMessage>,
}
struct State {
    session: Session,
    review: Option<Review>,
    busy: bool,
    pending: Option<Pending>,
    save_allowed: bool,
}
impl State {
    fn board(&self) -> Option<&Board> {
        self.review
            .as_ref()
            .map(|r| &r.board)
            .or(self.session.game.board.as_ref())
    }
    fn can_paste(&self) -> bool {
        !self.busy && self.board().is_none()
    }
}

pub struct Desktop {
    window: adw::ApplicationWindow,
    state: RefCell<State>,
    refreshing: Cell<bool>,
    drag: RefCell<Option<BoardDrag>>,
    save_dir: PathBuf,
    new_puzzle: gtk::Button,
    solved_new_puzzle: gtk::Button,
    paste: gtk::Button,
    heading: gtk::Label,
    count: gtk::Label,
    empty: adw::StatusPage,
    workspace: gtk::Box,
    play: gtk::Box,
    board: gtk::DrawingArea,
    hint: gtk::Button,
    apply_hint: gtk::Button,
    explain: gtk::Button,
    back_to_puzzle: gtk::Button,
    undo: gtk::Button,
    redo: gtk::Button,
    #[cfg(feature = "capture")]
    controls: gtk::MenuButton,
    #[cfg(feature = "capture")]
    menu: gtk::MenuButton,
    auto_crosses: gio::SimpleAction,
    show_errors: gio::SimpleAction,
    actions: gtk::Box,
    review_controls: gtk::Box,
    source_toggle: gtk::CheckButton,
    region_toggle: gtk::CheckButton,
    region: gtk::DropDown,
    accept: gtk::Button,
    cancel: gtk::Button,
    spinner: gtk::Spinner,
    status_frame: gtk::Frame,
    advice_scroll: gtk::ScrolledWindow,
    status_top: gtk::Box,
    status_heading: gtk::Label,
    message: gtk::Label,
    lesson_controls: gtk::Box,
    proof: gtk::Box,
    proof_previous: gtk::Button,
    proof_next: gtk::Button,
    proof_position: gtk::Label,
    conflicts: gtk::ListBox,
}

fn label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build()
}
fn margins(widget: &impl IsA<gtk::Widget>, amount: i32) {
    widget.set_margin_start(amount);
    widget.set_margin_end(amount);
    widget.set_margin_top(amount);
    widget.set_margin_bottom(amount);
}

fn action_tooltip(
    button: &gtk::Button,
    app: &adw::Application,
    action: &'static str,
    title: &'static str,
) {
    button.set_has_tooltip(true);
    let app = app.downgrade();
    button.connect_query_tooltip(move |_, _, _, _, tooltip| {
        let Some(app) = app.upgrade() else {
            return false;
        };
        // Read GTK's active bindings when the tooltip opens, so the label
        // cannot drift from the shortcut that actually activates the action.
        let shortcuts = app
            .accels_for_action(action)
            .iter()
            .filter_map(gtk::accelerator_parse)
            .map(|(key, modifiers)| gtk::accelerator_get_label(key, modifiers).to_string())
            .collect::<Vec<_>>()
            .join(" / ");
        let text = if shortcuts.is_empty() {
            title.to_owned()
        } else {
            format!("{title} ({shortcuts})")
        };
        tooltip.set_text(Some(&text));
        true
    });
}

fn set_advice_text(label: &gtk::Label, text: &str, size: usize) {
    label.set_text(text);
    let attributes = gtk::pango::AttrList::new();
    let mut offset = 0;
    // Pango attributes use byte offsets. Keep the original text intact, and
    // emphasize only complete, valid cell names (including two-digit columns).
    for part in text.split_inclusive(|c: char| !c.is_ascii_alphanumeric()) {
        let word = part.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
        if word
            .as_bytes()
            .first()
            .is_some_and(|&row| (b'A'..b'A' + size as u8).contains(&row))
            && word[1..]
                .parse::<usize>()
                .is_ok_and(|column| (1..=size).contains(&column))
        {
            let mut weight = gtk::pango::AttrInt::new_weight(gtk::pango::Weight::Bold);
            weight.set_start_index(offset as u32);
            weight.set_end_index((offset + word.len()) as u32);
            attributes.insert(weight);
        }
        offset += part.len();
    }
    label.set_attributes(Some(&attributes));
}

impl Desktop {
    pub fn open(app: &adw::Application) -> Rc<Self> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Learn Star Battle")
            .default_width(1040)
            .default_height(740)
            .build();
        window.set_size_request(700, 540);
        let header = adw::HeaderBar::new();
        let new_puzzle = gtk::Button::from_icon_name("document-new-symbolic");
        new_puzzle.set_tooltip_text(Some("New Puzzle"));
        new_puzzle.update_property(&[gtk::accessible::Property::Label("New Puzzle")]);
        header.pack_start(&new_puzzle);
        let history_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let undo = gtk::Button::from_icon_name("edit-undo-symbolic");
        undo.update_property(&[gtk::accessible::Property::Label("Undo")]);
        let redo = gtk::Button::from_icon_name("edit-redo-symbolic");
        redo.update_property(&[gtk::accessible::Property::Label("Redo")]);
        history_buttons.append(&undo);
        history_buttons.append(&redo);
        header.pack_start(&history_buttons);
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("open-menu-symbolic");
        menu.set_always_show_arrow(false);
        menu.set_tooltip_text(Some("Main Menu"));
        menu.update_property(&[gtk::accessible::Property::Label("Main Menu")]);
        let menu_model = gio::Menu::new();
        let preferences = gio::Menu::new();
        preferences.append(Some("Auto-Place X's"), Some("win.auto-crosses"));
        preferences.append(Some("Show Errors Automatically"), Some("win.show-errors"));
        menu_model.append_section(None, &preferences);
        let help = gio::Menu::new();
        help.append(Some("How to Play"), Some("win.help"));
        help.append(Some("About Learn Star Battle"), Some("win.about"));
        menu_model.append_section(None, &help);
        menu.set_menu_model(Some(&menu_model));
        header.pack_end(&menu);
        let auto_crosses =
            gio::SimpleAction::new_stateful("auto-crosses", None, &true.to_variant());
        let show_errors = gio::SimpleAction::new_stateful("show-errors", None, &true.to_variant());
        let toolbar = adw::ToolbarView::new();
        toolbar.set_top_bar_style(adw::ToolbarStyle::RaisedBorder);
        toolbar.add_top_bar(&header);
        window.set_content(Some(&toolbar));
        let clamp = adw::Clamp::builder()
            .maximum_size(1200)
            .tightening_threshold(1100)
            .build();
        toolbar.set_content(Some(&clamp));
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        margins(&content, 20);
        clamp.set_child(Some(&content));
        let empty = adw::StatusPage::builder().icon_name("edit-paste-symbolic")
            .title("Start With a Screenshot")
            .description("Copy a starbattlepuzzles.com board, then paste it here.\nInclude the whole board; existing stars and X's are kept.").build();
        let paste = gtk::Button::with_label("Paste Image");
        paste.add_css_class("suggested-action");
        paste.set_halign(gtk::Align::Center);
        empty.set_child(Some(&paste));
        content.append(&empty);
        let workspace = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        workspace.set_vexpand(true);
        content.append(&workspace);
        let play = gtk::Box::new(gtk::Orientation::Vertical, 10);
        play.set_hexpand(true);
        play.set_vexpand(true);
        workspace.append(&play);
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let heading = label("");
        heading.add_css_class("title-2");
        heading.set_hexpand(true);
        heading.set_wrap(false);
        let count = label("");
        count.set_wrap(false);
        top.append(&heading);
        top.append(&count);
        play.append(&top);
        let board = gtk::DrawingArea::builder()
            .content_width(260)
            .hexpand(true)
            .vexpand(true)
            .has_tooltip(true)
            .build();
        board.update_property(&[gtk::accessible::Property::Label("Star Battle board. Click a cell to cycle empty, X, star, empty. Drag to mark empty cells X, keeping existing marks.")]);
        play.append(&board);
        let review_controls = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let review_tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let source_toggle = gtk::CheckButton::with_label("Show Source");
        let region_toggle = gtk::CheckButton::with_label("Correct Regions");
        let region = gtk::DropDown::from_strings(&[]);
        review_tools.append(&source_toggle);
        review_tools.append(&region_toggle);
        review_tools.append(&region);
        review_controls.append(&review_tools);
        let review_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let accept = gtk::Button::with_label("Start Playing");
        accept.add_css_class("suggested-action");
        let cancel = gtk::Button::with_label("Cancel Import");
        review_buttons.append(&accept);
        review_buttons.append(&cancel);
        review_controls.append(&review_buttons);
        play.append(&review_controls);
        let actions = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let assistance = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        assistance.set_homogeneous(true);
        let hint = gtk::Button::with_label("Hint");
        hint.set_height_request(44);
        assistance.append(&hint);
        actions.append(&assistance);
        let controls = gtk::MenuButton::new();
        controls.set_icon_name("dialog-question-symbolic");
        controls.set_always_show_arrow(false);
        controls.add_css_class("flat");
        controls.update_property(&[gtk::accessible::Property::Label("How to Play")]);
        controls.set_tooltip_text(Some(
            &MOUSE_CONTROLS
                .map(|(input, action)| format!("{input}: {action}"))
                .join("\n"),
        ));
        let controls_popover = gtk::Popover::new();
        controls_popover.set_position(gtk::PositionType::Bottom);
        let controls_grid = gtk::Grid::new();
        controls_grid.set_row_spacing(8);
        controls_grid.set_column_spacing(16);
        margins(&controls_grid, 8);
        for (row, (input, action)) in MOUSE_CONTROLS.into_iter().enumerate() {
            let input = label(input);
            input.set_wrap(false);
            input.set_valign(gtk::Align::Baseline);
            input.add_css_class("heading");
            let action = label(action);
            action.set_valign(gtk::Align::Baseline);
            action.set_max_width_chars(32);
            controls_grid.set_row_baseline_position(row as i32, gtk::BaselinePosition::Top);
            controls_grid.attach(&input, 0, row as i32, 1, 1);
            controls_grid.attach(&action, 1, row as i32, 1, 1);
        }
        controls_popover.set_child(Some(&controls_grid));
        controls.set_popover(Some(&controls_popover));
        header.pack_end(&controls);
        play.append(&actions);
        let status = gtk::Box::new(gtk::Orientation::Vertical, 8);
        margins(&status, 14);
        let status_frame = gtk::Frame::new(None);
        // Only the window size determines the space for the board and advice.
        // Overflow stays in this panel and can never scroll the board away.
        let advice_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&status)
            .build();
        status.set_valign(gtk::Align::Start);
        let advice_panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
        advice_panel.append(&advice_scroll);
        let lesson_controls = gtk::Box::new(gtk::Orientation::Vertical, 8);
        margins(&lesson_controls, 14);
        lesson_controls.set_margin_top(0);
        advice_panel.append(&lesson_controls);
        status_frame.set_child(Some(&advice_panel));
        status_frame.set_width_request(340);
        status_frame.set_hexpand(false);
        status_frame.set_vexpand(false);
        workspace.append(&status_frame);
        // In a tall, narrow window the same panel fits under the board. Reserve
        // its height even before a hint is shown, so buttons never jump around.
        let narrow = adw::Breakpoint::new(
            adw::BreakpointCondition::parse("max-width: 800px and min-height: 800px").unwrap(),
        );
        narrow.add_setter(
            &workspace,
            "orientation",
            Some(&gtk::Orientation::Vertical.to_value()),
        );
        narrow.add_setter(&status_frame, "width-request", Some(&(-1_i32).to_value()));
        narrow.add_setter(&status_frame, "height-request", Some(&240_i32.to_value()));
        window.add_breakpoint(narrow);
        let status_top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let status_heading = label("");
        status_heading.add_css_class("heading");
        status_heading.set_max_width_chars(30);
        status_heading.set_hexpand(true);
        status_top.append(&status_heading);
        let spinner = gtk::Spinner::new();
        status_top.append(&spinner);
        status.append(&status_top);
        let message = label("");
        message.set_selectable(true);
        message.set_max_width_chars(36);
        status.append(&message);
        let solved_new_puzzle = gtk::Button::with_label("New Puzzle");
        solved_new_puzzle.add_css_class("suggested-action");
        solved_new_puzzle.set_halign(gtk::Align::Start);
        lesson_controls.append(&solved_new_puzzle);
        let conflicts = gtk::ListBox::new();
        conflicts.add_css_class("boxed-list");
        status.append(&conflicts);
        let explain = gtk::Button::with_label("Explain Hint");
        explain.set_halign(gtk::Align::Start);
        lesson_controls.append(&explain);
        let proof = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let proof_previous = gtk::Button::with_label("Previous");
        let proof_next = gtk::Button::with_label("Next");
        let navigation_size = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
        navigation_size.add_widget(&proof_previous);
        navigation_size.add_widget(&proof_next);
        let proof_position = label("");
        proof_position.set_wrap(false);
        proof_position.set_hexpand(true);
        proof_position.set_xalign(0.5);
        proof_position.add_css_class("dim-label");
        proof.append(&proof_previous);
        proof.append(&proof_position);
        proof.append(&proof_next);
        lesson_controls.append(&proof);
        let apply_hint = gtk::Button::with_label("Apply Hint");
        apply_hint.add_css_class("suggested-action");
        apply_hint.set_halign(gtk::Align::Start);
        lesson_controls.append(&apply_hint);
        let back_to_puzzle = gtk::Button::with_label("Back to Puzzle");
        back_to_puzzle.set_halign(gtk::Align::Start);
        lesson_controls.append(&back_to_puzzle);
        let action_size = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
        for button in [&solved_new_puzzle, &explain, &apply_hint, &back_to_puzzle] {
            action_size.add_widget(button);
        }
        let save_dir = storage::directory();
        let loaded = storage::load(&save_dir);
        let mut session = Session::new(loaded.as_ref().cloned().unwrap_or_default());
        if let Err(error) = &loaded {
            session.message = format!("Could not load your saved game: {error}");
            session.message_error = true;
        }
        let desktop = Rc::new(Self {
            window,
            state: RefCell::new(State {
                session,
                review: None,
                busy: false,
                pending: None,
                save_allowed: loaded.is_ok(),
            }),
            refreshing: Cell::new(false),
            drag: RefCell::new(None),
            save_dir,
            new_puzzle,
            solved_new_puzzle,
            paste,
            heading,
            count,
            empty,
            workspace,
            play,
            board,
            hint,
            apply_hint,
            explain,
            back_to_puzzle,
            undo,
            redo,
            #[cfg(feature = "capture")]
            controls,
            #[cfg(feature = "capture")]
            menu,
            auto_crosses,
            show_errors,
            actions,
            review_controls,
            source_toggle,
            region_toggle,
            region,
            accept,
            cancel,
            spinner,
            status_frame,
            advice_scroll,
            status_top,
            status_heading,
            message,
            lesson_controls,
            proof,
            proof_previous,
            proof_next,
            proof_position,
            conflicts,
        });
        desktop.connect(app);
        desktop.refresh();
        desktop.window.present();
        #[cfg(feature = "capture")]
        desktop.validation_capture();
        // Keep the controller alive for this application's lifetime. Callbacks
        // otherwise hold Weak references, including the background-job poller.
        let save = desktop.clone();
        app.connect_shutdown(move |_| save.persist());
        desktop
    }

    pub fn present(&self) {
        self.window.present();
    }

    pub fn open_files(self: &Rc<Self>, files: &[gio::File]) {
        let state = self.state.borrow();
        let problem = if files.len() != 1 {
            Some("Open one screenshot at a time.")
        } else if state.busy || state.review.is_some() {
            Some(
                "Finish the current operation or cancel the import before opening another screenshot.",
            )
        } else if files[0].path().is_none() {
            Some("Save the screenshot to your computer before opening it.")
        } else {
            None
        };
        drop(state);
        if let Some(message) = problem {
            let dialog = adw::MessageDialog::new(
                Some(&self.window),
                Some("Cannot Open Screenshot"),
                Some(message),
            );
            dialog.add_response("close", "_Close");
            dialog.set_close_response("close");
            dialog.set_default_response(Some("close"));
            dialog.present();
        } else {
            // Review is reversible: the saved game is replaced only when the
            // player accepts this import. Cancel restores the existing puzzle.
            self.import(files[0].path());
        }
    }

    fn connect(self: &Rc<Self>, app: &adw::Application) {
        for (button, forward) in [(&self.proof_previous, false), (&self.proof_next, true)] {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(ui) = weak.upgrade() {
                    ui.state.borrow_mut().session.navigate_explanation(forward);
                    ui.refresh();
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.back_to_puzzle.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.state.borrow_mut().session.leave_explanation();
                ui.refresh();
            }
        });
        let weak = Rc::downgrade(self);
        self.apply_hint.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.state.borrow_mut().session.apply_hint();
                ui.persist();
                ui.refresh();
            }
        });
        for button in [&self.new_puzzle, &self.solved_new_puzzle] {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(ui) = weak.upgrade() {
                    ui.confirm_new_puzzle();
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.auto_crosses.connect_change_state(move |_, value| {
            if let Some(ui) = weak.upgrade() {
                let Some(enabled) = value.and_then(|value| value.get::<bool>()) else {
                    return;
                };
                ui.state.borrow_mut().session.set_auto_crosses(enabled);
                ui.persist();
                ui.refresh();
            }
        });
        self.window.add_action(&self.auto_crosses);
        let weak = Rc::downgrade(self);
        self.show_errors.connect_change_state(move |_, value| {
            if let Some(ui) = weak.upgrade() {
                let Some(enabled) = value.and_then(|value| value.get::<bool>()) else {
                    return;
                };
                ui.state.borrow_mut().session.game.show_errors = enabled;
                ui.persist();
                ui.refresh();
            }
        });
        self.window.add_action(&self.show_errors);
        for name in ["help", "about"] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(ui) = weak.upgrade() {
                    if name == "help" {
                        ui.show_help();
                    } else {
                        ui.show_about();
                    }
                }
            });
            self.window.add_action(&action);
        }
        let weak = Rc::downgrade(self);
        self.paste.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.import(None);
            }
        });
        for (button, preview) in [(&self.hint, false), (&self.explain, true)] {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(ui) = weak.upgrade() {
                    ui.request_advice(preview);
                }
            });
        }
        for (button, redo) in [(&self.undo, false), (&self.redo, true)] {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(ui) = weak.upgrade() {
                    ui.history(redo);
                }
            });
        }
        for (name, accelerators) in [
            ("paste", vec!["<Primary>v"]),
            ("undo", vec!["<Primary>z"]),
            ("redo", vec!["<Primary><Shift>z"]),
        ] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(ui) = weak.upgrade() {
                    match name {
                        "paste" => ui.import(None),
                        "undo" => ui.history(false),
                        _ => ui.history(true),
                    }
                }
            });
            self.window.add_action(&action);
            app.set_accels_for_action(&format!("win.{name}"), &accelerators);
        }
        for (button, action, title) in [
            (&self.undo, "win.undo", "Undo"),
            (&self.redo, "win.redo", "Redo"),
            (&self.paste, "win.paste", "Paste a Board Screenshot"),
        ] {
            action_tooltip(button, app, action, title);
        }
        let weak = Rc::downgrade(self);
        self.board.set_draw_func(move |area, cr, width, height| {
            if let Some(ui) = weak.upgrade() {
                let state = ui.state.borrow();
                if let Some(board) = state.board() {
                    let source = state
                        .review
                        .as_ref()
                        .filter(|_| ui.source_toggle.is_active())
                        .map(|review| &review.source);
                    draw_board(area, cr, width, height, board, &state.session, source);
                }
            }
        });
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        let weak = Rc::downgrade(self);
        drag.connect_drag_begin(move |gesture, x, y| {
            if let Some(ui) = weak.upgrade() {
                if ui.begin_board_drag(x, y) {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_update(move |_, x, y| {
            if let Some(ui) = weak.upgrade() {
                ui.update_board_drag(x, y);
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_end(move |_, x, y| {
            if let Some(ui) = weak.upgrade() {
                ui.finish_board_drag(x, y);
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_cancel(move |_, _| {
            if let Some(ui) = weak.upgrade() {
                ui.drag.borrow_mut().take();
                ui.state.borrow_mut().session.cancel_cross_stroke();
                ui.refresh();
            }
        });
        self.board.add_controller(drag);
        let weak = Rc::downgrade(self);
        self.board
            .connect_query_tooltip(move |area, x, y, _, tooltip| {
                let Some(ui) = weak.upgrade() else {
                    return false;
                };
                let state = ui.state.borrow();
                if state.review.is_some() && ui.source_toggle.is_active() {
                    return false;
                }
                let Some(b) = state.board() else {
                    return false;
                };
                let Some(i) =
                    Geometry::new(area.width(), area.height(), b.size).cell_at(x as f64, y as f64)
                else {
                    return false;
                };
                tooltip.set_text(Some(&format!(
                    "{} · Region {}",
                    b.address(i),
                    b.regions[i] + 1
                )));
                true
            });
        let weak = Rc::downgrade(self);
        self.source_toggle.connect_toggled(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.board.queue_draw();
            }
        });
        let weak = Rc::downgrade(self);
        self.region_toggle.connect_toggled(move |button| {
            if let Some(ui) = weak.upgrade() {
                ui.region.set_visible(button.is_active());
            }
        });
        let weak = Rc::downgrade(self);
        self.accept.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                let mut state = ui.state.borrow_mut();
                if state
                    .review
                    .as_ref()
                    .is_some_and(|r| r.board.validate().is_ok())
                {
                    let review = state.review.take().unwrap();
                    state.session.game.load(review.board);
                    state.session.reset();
                    state.save_allowed = true;
                }
                drop(state);
                ui.persist();
                ui.refresh();
            }
        });
        let weak = Rc::downgrade(self);
        self.cancel.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                let mut state = ui.state.borrow_mut();
                state.review = None;
                state.session.changed();
                drop(state);
                ui.refresh();
            }
        });
        let weak = Rc::downgrade(self);
        self.conflicts.connect_row_selected(move |_, row| {
            if let Some(ui) = weak.upgrade() {
                if ui.refreshing.get() {
                    return;
                }
                if let Some(row) = row {
                    {
                        let mut state = ui.state.borrow_mut();
                        state.session.select_conflict(row.index() as usize);
                    }
                    ui.refresh();
                }
            }
        });
        let weak = Rc::downgrade(self);
        adw::StyleManager::default().connect_dark_notify(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.board.queue_draw();
            }
        });
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(70), move || {
            let Some(ui) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            ui.poll();
            glib::ControlFlow::Continue
        });
    }

    fn refresh(&self) {
        self.refreshing.set(true);
        let state = self.state.borrow();
        let session = &state.session;
        let review = state.review.is_some();
        let board = state.board();
        self.empty.set_visible(board.is_none());
        self.play.set_visible(board.is_some());
        self.new_puzzle
            .set_sensitive(!state.busy && !review && board.is_some());
        self.solved_new_puzzle
            .set_visible(!review && board.is_some_and(Board::solved));
        self.solved_new_puzzle.set_sensitive(!state.busy);
        self.paste.set_sensitive(state.can_paste());
        let show_advice = board.is_some() || state.busy || session.message_error;
        self.status_frame.set_visible(show_advice);
        self.workspace.set_visible(show_advice);
        self.spinner.set_visible(state.busy);
        self.spinner.set_spinning(state.busy);
        self.review_controls.set_visible(review);
        self.actions.set_visible(!review);
        self.region
            .set_visible(review && self.region_toggle.is_active());
        let hypothetical = session.lesson_frame().is_some();
        self.hint
            .set_sensitive(!state.busy && !review && session.can_request_hint());
        self.explain.set_visible(
            !review
                && session.can_explain()
                && !session.can_apply_hint()
                && session.lesson_index.is_none(),
        );
        self.explain.set_sensitive(!state.busy);
        self.apply_hint
            .set_visible(!review && session.can_apply_hint());
        self.apply_hint
            .set_sensitive(!state.busy && session.can_apply_hint());
        self.apply_hint.set_tooltip_text(Some(
            "Apply the remaining suggested marks as one undoable action",
        ));
        self.back_to_puzzle.set_visible(hypothetical);
        self.lesson_controls.set_visible(
            !review
                && (session.can_explain()
                    || session.lesson_index.is_some()
                    || board.is_some_and(Board::solved)),
        );
        self.proof
            .set_visible(!review && session.lesson_index.is_some());
        if let Some(index) = session.lesson_index {
            self.proof_position
                .set_text(&format!("{} of {}", index + 1, session.lesson_length()));
            self.proof_previous.set_sensitive(index > 0);
            self.proof_next
                .set_sensitive(index + 1 < session.lesson_length());
        }
        self.undo
            .set_sensitive(!review && !hypothetical && session.game.can_undo());
        self.redo
            .set_sensitive(!review && !hypothetical && session.game.can_redo());
        for (name, button) in [
            ("paste", &self.paste),
            ("undo", &self.undo),
            ("redo", &self.redo),
        ] {
            if let Some(action) = self.window.lookup_action(name) {
                action
                    .downcast::<gio::SimpleAction>()
                    .unwrap()
                    .set_enabled(button.is_sensitive());
            }
        }
        self.auto_crosses.set_enabled(!state.busy && !hypothetical);
        self.auto_crosses
            .set_state(&session.game.auto_crosses.to_variant());
        self.show_errors
            .set_state(&session.game.show_errors.to_variant());
        if let Some(board) = board {
            self.heading.set_text(&format!(
                "{} × {} · {} {}",
                board.size,
                board.size,
                board.required(),
                if board.required() == 1 {
                    "star"
                } else {
                    "stars"
                }
            ));
            self.heading.set_tooltip_text(Some(&format!(
                "Exactly {} {} in every row, column, and region. Stars cannot touch, even diagonally.",
                board.required(),
                if board.required() == 1 { "star" } else { "stars" }
            )));
            self.count.set_text(&format!(
                "{} / {} stars",
                board.marks.iter().filter(|&&m| m == Mark::Star).count(),
                board.size * board.required()
            ));
        }
        let violations = board
            .filter(|_| session.game.show_errors && !hypothetical)
            .map(Board::violations)
            .unwrap_or_default();
        while let Some(child) = self.conflicts.first_child() {
            self.conflicts.remove(&child);
        }
        self.conflicts.set_visible(!violations.is_empty());
        for violation in &violations {
            let row = label(&violation.message);
            row.set_max_width_chars(32);
            margins(&row, 10);
            self.conflicts.append(&row);
        }
        if !violations.is_empty() && session.error_cells.is_empty() {
            self.conflicts.select_row(
                self.conflicts
                    .row_at_index(session.conflict.min(violations.len() - 1) as i32)
                    .as_ref(),
            );
        } else {
            self.conflicts.unselect_all();
        }
        self.message.remove_css_class("error");
        let (title, text) = if let Some(review) = &state.review {
            let valid = review.board.validate();
            self.accept.set_sensitive(valid.is_ok());
            (
                "Review Screenshot",
                match valid {
                    Err(error) => error,
                    Ok(()) => format!(
                        "Check the regions and marks, then choose Start Playing. {}",
                        review.warnings.join(" ")
                    ),
                },
            )
        } else if let Some(frame) = session.lesson_frame() {
            ("Trial · Puzzle Unchanged", frame.text.clone())
        } else if let Some(hint_text) = session.hint_text() {
            (
                if session.can_apply_hint() && session.message_error {
                    "Correction"
                } else if session.can_apply_hint() {
                    "Conclusion"
                } else if !session.error_cells.is_empty() {
                    "Hint · Check the Highlighted Symbols"
                } else {
                    "Hint"
                },
                if session.message != hint_text {
                    format!("{hint_text}\n\n{}", session.message)
                } else {
                    hint_text.to_owned()
                },
            )
        } else if session.message_error {
            (
                if session.preview {
                    "Correction Preview · No Marks Changed"
                } else if !session.error_cells.is_empty() {
                    "Hint · Check the Highlighted Symbols"
                } else {
                    "Unable to Continue"
                },
                session.message.clone(),
            )
        } else if !violations.is_empty() {
            ("Conflicts", "Select a message below. Red shading and outlines show exactly the cells affected by that rule.".into())
        } else if board.is_some_and(Board::solved) {
            (
                "Puzzle Solved",
                "All stars are in place and none touch.".into(),
            )
        } else {
            (
                if state.busy {
                    "Analyzing…"
                } else if session.preview {
                    "Conclusion"
                } else if session.advice.is_some() {
                    "Hint"
                } else {
                    ""
                },
                session.message.clone(),
            )
        };
        self.status_heading.set_text(title);
        self.status_heading.set_visible(!title.is_empty());
        self.status_top.set_visible(!title.is_empty() || state.busy);
        let board_size = board.map_or(0, |board| board.size);
        if self.message.text() != text {
            self.advice_scroll.vadjustment().set_value(0.);
        }
        set_advice_text(&self.message, &text, board_size);
        if session.message_error {
            self.message.add_css_class("error");
        }
        self.message.set_visible(!text.is_empty());
        self.board.queue_draw();
        self.refreshing.set(false);
    }

    fn persist(&self) {
        let mut state = self.state.borrow_mut();
        if !state.save_allowed {
            return;
        }
        if let Err(error) = storage::save(&self.save_dir, &state.session.game) {
            state.session.message = format!("Could not save this game: {error}");
            state.session.message_error = true;
        }
    }
    fn history(&self, redo: bool) {
        let mut state = self.state.borrow_mut();
        if state.review.is_some() || state.session.lesson_frame().is_some() {
            return;
        }
        if redo {
            state.session.game.redo();
        } else {
            state.session.game.undo();
        }
        state.session.changed();
        drop(state);
        self.persist();
        self.refresh();
    }
    fn show_help(&self) {
        let dialog = adw::MessageDialog::new(
            Some(&self.window),
            Some("How to Play"),
            Some(
                "Place the indicated number of stars in every row, column, and region. Stars cannot touch, even diagonally.\n\nClick a cell to cycle empty → X → star → empty. Drag to mark empty cells X; existing marks stay.\n\nHint points to a deduction. Choose Explain Hint in the advice panel to explore it. Previous and Next step through longer explanations, with the board showing each deduction.\n\nHypothetical steps are read-only and do not change your puzzle. Back to Puzzle exits the lesson. At the conclusion, choose Apply Hint or place the suggested marks yourself.\n\nAuto-Place X's in the main menu marks neighbors of stars and completed rows, columns, or regions. Removing a star clears automatic X's that are no longer needed; manual marks stay.",
            ),
        );
        dialog.add_response("close", "_Close");
        dialog.set_default_response(Some("close"));
        dialog.set_close_response("close");
        dialog.present();
    }

    fn show_about(&self) {
        adw::AboutWindow::builder()
            .transient_for(&self.window)
            .modal(true)
            .application_name("Learn Star Battle")
            .application_icon("io.github.starbattle.Solver")
            .version(env!("CARGO_PKG_VERSION"))
            .comments("Play Star Battle puzzles from screenshots, with hints that follow the current board.")
            .build()
            .present();
    }

    fn confirm_new_puzzle(self: &Rc<Self>) {
        let state = self.state.borrow();
        if state.busy || state.review.is_some() || state.board().is_none() {
            return;
        }
        if state.board().is_some_and(Board::solved) {
            drop(state);
            self.start_new_puzzle();
            return;
        }
        drop(state);
        let dialog = adw::MessageDialog::new(
            Some(&self.window),
            Some("Start a new puzzle?"),
            Some(
                "This clears the current puzzle and its undo history so you can paste a new board. Your settings will be kept.",
            ),
        );
        dialog.add_responses(&[("cancel", "_Cancel"), ("new", "_New Puzzle")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("new", adw::ResponseAppearance::Destructive);
        let weak = Rc::downgrade(self);
        dialog.connect_response(Some("new"), move |_, _| {
            if let Some(ui) = weak.upgrade() {
                ui.start_new_puzzle();
            }
        });
        dialog.present();
    }
    fn start_new_puzzle(&self) {
        let mut state = self.state.borrow_mut();
        state.session.new_puzzle();
        state.pending = None;
        state.busy = false;
        self.drag.borrow_mut().take();
        drop(state);
        self.persist();
        self.refresh();
    }
    fn edit(&self, index: usize, mark: Option<Mark>) {
        let mut state = self.state.borrow_mut();
        if let Some(review) = &mut state.review {
            if self.region_toggle.is_active() {
                review.board.regions[index] = self.region.selected() as usize;
            } else {
                review.board.marks[index] =
                    mark.unwrap_or_else(|| review.board.marks[index].next());
            }
            state.session.changed();
        } else {
            state.session.edit(index, mark);
        }
        drop(state);
        self.persist();
        self.refresh();
    }
    fn begin_board_drag(&self, x: f64, y: f64) -> bool {
        let state = self.state.borrow();
        if state.session.lesson_frame().is_some()
            || (state.review.is_some() && self.source_toggle.is_active())
        {
            return false;
        }
        let Some(index) = state.board().and_then(|b| {
            Geometry::new(self.board.width(), self.board.height(), b.size).cell_at(x, y)
        }) else {
            return false;
        };
        *self.drag.borrow_mut() = Some(BoardDrag {
            start: (x, y),
            last: (x, y),
            cell: index,
            painting: false,
            revision: state.session.revision,
        });
        true
    }

    fn update_board_drag(&self, dx: f64, dy: f64) {
        let mut drag = self.drag.borrow_mut();
        let Some(stroke) = drag.as_mut() else {
            return;
        };
        let mut state = self.state.borrow_mut();
        if stroke.revision != state.session.revision {
            return;
        }
        let end = (stroke.start.0 + dx, stroke.start.1 + dy);
        let started = !stroke.painting
            && self.board.drag_check_threshold(
                stroke.start.0 as i32,
                stroke.start.1 as i32,
                end.0 as i32,
                end.1 as i32,
            );
        if started {
            stroke.painting = true;
            if state.review.is_none() {
                state.session.begin_cross_stroke();
                stroke.revision = state.session.revision;
            }
        }
        if stroke.painting && state.review.is_none() {
            if let Some(board) = state.board() {
                let cells = Geometry::new(self.board.width(), self.board.height(), board.size)
                    .cells_along(stroke.last, end);
                state.session.extend_cross_stroke(&cells);
            }
            stroke.last = end;
        }
        drop(state);
        drop(drag);
        if started {
            self.refresh();
        } else {
            self.board.queue_draw();
        }
    }

    fn finish_board_drag(&self, dx: f64, dy: f64) {
        self.update_board_drag(dx, dy);
        let Some(stroke) = self.drag.borrow_mut().take() else {
            return;
        };
        let mut state = self.state.borrow_mut();
        if stroke.revision != state.session.revision {
            return;
        }
        if stroke.painting {
            state.session.finish_cross_stroke();
            drop(state);
            self.persist();
            self.refresh();
        } else {
            drop(state);
            self.edit(stroke.cell, None);
        }
    }

    fn request_advice(&self, preview: bool) {
        let mut state = self.state.borrow_mut();
        if state.busy || state.review.is_some() {
            return;
        }
        // Guard the operation as well as the disabled button: an active hint
        // must not be replaced or restarted by a second Hint request.
        if !preview && !state.session.can_request_hint() {
            return;
        }
        if state.session.advice.is_some() {
            if preview {
                state.session.start_explanation();
            } else {
                state.session.leave_explanation();
                state.session.reveal(false);
            }
            drop(state);
            self.refresh();
            return;
        }
        let Some(board) = state.session.game.board.clone() else {
            return;
        };
        let revision = state.session.revision;
        let (tx, rx) = mpsc::channel();
        state.pending = Some(Pending {
            revision,
            receiver: rx,
        });
        state.busy = true;
        if !state.session.has_hint() {
            state.session.message = "Looking for a hint…".into();
            state.session.message_error = false;
        }
        drop(state);
        self.refresh();
        std::thread::spawn(move || {
            let advice = advise(&board, Duration::from_secs(8));
            let _ = tx.send(ResultMessage::Advised(advice, preview));
        });
    }
    fn import(self: &Rc<Self>, path: Option<PathBuf>) {
        let mut state = self.state.borrow_mut();
        // Guard the operation as well as the buttons and application shortcut.
        // Opening an explicit file on launch remains a separate import path.
        if state.busy || (path.is_none() && !state.can_paste()) {
            return;
        }
        state.busy = true;
        if !state.session.has_hint() {
            state.session.message = "Reading the screenshot…".into();
            state.session.message_error = false;
        }
        let revision = state.session.revision;
        drop(state);
        self.refresh();
        if let Some(path) = path {
            self.import_worker(revision, move || {
                image::open(path).map_err(|e| e.to_string())
            });
            return;
        }
        let weak = Rc::downgrade(self);
        let clipboard = self.window.clipboard();
        glib::MainContext::default().spawn_local(async move {
            let result = clipboard.read_texture_future().await;
            let Some(ui) = weak.upgrade() else { return; };
            if ui.state.borrow().session.revision != revision { ui.state.borrow_mut().busy = false; ui.refresh(); return; }
            match result {
                Ok(Some(texture)) if texture.width() as i64 * texture.height() as i64 <= 40_000_000 => {
                    let bytes = texture.save_to_png_bytes().to_vec();
                    ui.import_worker(revision, move || image::load_from_memory(&bytes).map_err(|e| e.to_string()));
                }
                other => {
                    let mut state = ui.state.borrow_mut(); state.busy = false; state.session.message_error = true;
                    state.session.message = match other {
                        Ok(Some(_)) => "Screenshot is too large. Crop it to the board first.".into(),
                        Err(error) => format!("No readable image in the clipboard. Copy a screenshot and choose Paste Image. ({error})"),
                        _ => "No image in the clipboard. Copy a screenshot and choose Paste Image.".into(),
                    };
                    drop(state); ui.refresh();
                }
            }
        });
    }
    fn import_worker(
        &self,
        revision: u64,
        read: impl FnOnce() -> Result<image::DynamicImage, String> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        self.state.borrow_mut().pending = Some(Pending {
            revision,
            receiver: rx,
        });
        std::thread::spawn(move || {
            let result = read().and_then(|image| {
                if image.width() as u64 * image.height() as u64 > 40_000_000 {
                    return Err("Screenshot is too large; crop it to the board first.".into());
                }
                let image = image.to_rgb8();
                let recognized = recognize(&image)?;
                let [x, y, w, h] = recognized.crop;
                let crop = image::imageops::crop_imm(&image, x, y, w, h).to_image();
                Ok((recognized, crop))
            });
            let _ = tx.send(ResultMessage::Imported(result));
        });
    }
    fn poll(&self) {
        let mut state = self.state.borrow_mut();
        let Some(pending) = &state.pending else {
            return;
        };
        let revision = pending.revision;
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                state.pending = None;
                state.busy = false;
                state.session.message = "The operation could not finish. Please try again.".into();
                state.session.message_error = true;
                drop(state);
                self.refresh();
                return;
            }
        };
        state.pending = None;
        state.busy = false;
        if revision != state.session.revision {
            drop(state);
            self.refresh();
            return;
        }
        match result {
            ResultMessage::Advised(advice, preview) => {
                state.session.receive(revision, advice, false);
                if preview {
                    state.session.start_explanation();
                }
            }
            ResultMessage::Imported(Err(error)) => {
                state.session.message = error;
                state.session.message_error = true;
            }
            ResultMessage::Imported(Ok((recognized, crop))) => {
                let source = gtk::gdk_pixbuf::Pixbuf::from_bytes(
                    &glib::Bytes::from_owned(crop.as_raw().clone()),
                    gtk::gdk_pixbuf::Colorspace::Rgb,
                    false,
                    8,
                    crop.width() as i32,
                    crop.height() as i32,
                    crop.width() as i32 * 3,
                );
                let names: Vec<_> = (1..=recognized.board.size)
                    .map(|i| format!("Region {i}"))
                    .collect();
                self.region.set_model(Some(&gtk::StringList::new(
                    &names.iter().map(String::as_str).collect::<Vec<_>>(),
                )));
                self.region.set_selected(0);
                self.source_toggle.set_active(false);
                self.region_toggle.set_active(false);
                state.review = Some(Review {
                    board: recognized.board,
                    warnings: recognized.warnings,
                    source,
                });
                state.session.changed();
            }
        }
        drop(state);
        self.refresh();
    }
}

#[derive(Clone, Copy)]
struct BoardDrag {
    start: (f64, f64),
    last: (f64, f64),
    cell: usize,
    painting: bool,
    revision: u64,
}

#[derive(Clone, Copy)]
struct Geometry {
    left: f64,
    top: f64,
    cell: f64,
    size: usize,
}
impl Geometry {
    fn new(width: i32, height: i32, size: usize) -> Self {
        let side = (width as f64 - 38.)
            .min(height as f64 - 32.)
            .clamp(1., 560.);
        Self {
            left: (width as f64 - side) / 2. + 8.,
            top: 24.,
            cell: side / size as f64,
            size,
        }
    }
    fn cell_at(self, x: f64, y: f64) -> Option<usize> {
        let c = (x - self.left) / self.cell;
        let r = (y - self.top) / self.cell;
        (c >= 0. && r >= 0. && c < self.size as f64 && r < self.size as f64)
            .then(|| r as usize * self.size + c as usize)
    }
    fn origin(self, i: usize) -> (f64, f64) {
        (
            self.left + (i % self.size) as f64 * self.cell,
            self.top + (i / self.size) as f64 * self.cell,
        )
    }

    // Split the pointer segment at grid lines so even a fast swipe visits every
    // crossed square. Sampling each interior interval avoids painting squares
    // merely touched at a diagonal corner, and naturally clips off-board motion.
    fn cells_along(self, from: (f64, f64), to: (f64, f64)) -> Vec<usize> {
        let mut cuts = vec![0., 1.];
        for (a, b, origin) in [(from.0, to.0, self.left), (from.1, to.1, self.top)] {
            if a != b {
                for i in 0..=self.size {
                    let t = (origin + i as f64 * self.cell - a) / (b - a);
                    if t > 0. && t < 1. {
                        cuts.push(t);
                    }
                }
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        let mut cells = vec![];
        for t in std::iter::once(0.)
            .chain(cuts.windows(2).map(|pair| (pair[0] + pair[1]) / 2.))
            .chain(std::iter::once(1.))
        {
            if let Some(i) =
                self.cell_at(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
            {
                if cells.last() != Some(&i) {
                    cells.push(i);
                }
            }
        }
        cells
    }
}
fn color(cr: &cairo::Context, rgb: (f64, f64, f64)) {
    cr.set_source_rgb(rgb.0, rgb.1, rgb.2);
}
fn line(cr: &cairo::Context, a: (f64, f64), b: (f64, f64)) {
    cr.move_to(a.0, a.1);
    cr.line_to(b.0, b.1);
}
fn draw_hint_outline(cr: &cairo::Context, g: Geometry, cells: &[usize]) {
    if cells.is_empty() {
        return;
    }
    let _ = cr.save();
    let inset = (g.cell * 0.12).min(5.);
    let contains = |r: isize, c: isize| {
        r >= 0
            && c >= 0
            && r < g.size as isize
            && c < g.size as isize
            && cells.contains(&(r as usize * g.size + c as usize))
    };
    // Shorten convex corners and extend concave corners so inset exposed edges
    // still meet. Straight edges continue across cell boundaries unchanged.
    let corner = |adjacent: bool, diagonal: bool| {
        if !adjacent {
            inset
        } else if diagonal {
            -inset
        } else {
            0.
        }
    };
    cr.set_line_cap(cairo::LineCap::Round);
    for &i in cells {
        let (x, y) = g.origin(i);
        let (r, c) = ((i / g.size) as isize, (i % g.size) as isize);
        let (left, right) = (contains(r, c - 1), contains(r, c + 1));
        let (top, bottom) = (contains(r - 1, c), contains(r + 1, c));
        if !left {
            line(
                cr,
                (x + inset, y + corner(top, contains(r - 1, c - 1))),
                (
                    x + inset,
                    y + g.cell - corner(bottom, contains(r + 1, c - 1)),
                ),
            );
        }
        if !right {
            line(
                cr,
                (x + g.cell - inset, y + corner(top, contains(r - 1, c + 1))),
                (
                    x + g.cell - inset,
                    y + g.cell - corner(bottom, contains(r + 1, c + 1)),
                ),
            );
        }
        if !top {
            line(
                cr,
                (x + corner(left, contains(r - 1, c - 1)), y + inset),
                (
                    x + g.cell - corner(right, contains(r - 1, c + 1)),
                    y + inset,
                ),
            );
        }
        if !bottom {
            line(
                cr,
                (x + corner(left, contains(r + 1, c - 1)), y + g.cell - inset),
                (
                    x + g.cell - corner(right, contains(r + 1, c + 1)),
                    y + g.cell - inset,
                ),
            );
        }
    }
    // Keep the light backing and bright stroke inside the group, leaving the
    // original black region borders visible beside them.
    color(cr, (1., 248. / 255., 230. / 255.));
    cr.set_line_width((g.cell * 0.124).min(6.2));
    let _ = cr.stroke_preserve();
    color(cr, ORANGE);
    cr.set_line_width((g.cell * 0.076).min(3.8));
    let _ = cr.stroke();
    let _ = cr.restore();
}
fn draw_mark(cr: &cairo::Context, mark: Mark, x: f64, y: f64, cell: f64, rgb: (f64, f64, f64)) {
    color(cr, rgb);
    match mark {
        Mark::Star => {
            for p in 0..10 {
                let angle = p as f64 * std::f64::consts::PI / 5. - std::f64::consts::FRAC_PI_2;
                let radius = cell * if p % 2 == 0 { 0.29 } else { 0.13 };
                let (xx, yy) = (x + angle.cos() * radius, y + angle.sin() * radius);
                if p == 0 {
                    cr.move_to(xx, yy);
                } else {
                    cr.line_to(xx, yy);
                }
            }
            cr.close_path();
            let _ = cr.fill();
        }
        Mark::Cross => {
            let d = cell * 0.13;
            cr.set_line_width(1.7);
            line(cr, (x - d, y - d), (x + d, y + d));
            line(cr, (x - d, y + d), (x + d, y - d));
            let _ = cr.stroke();
        }
        Mark::Empty => {}
    }
}
fn draw_suggestion(
    cr: &cairo::Context,
    mark: Mark,
    x: f64,
    y: f64,
    cell: f64,
    ink: (f64, f64, f64),
    dark: bool,
) {
    let _ = cr.save();
    let inset = (cell * 0.24).min(12.);
    let side = cell - inset * 2.;
    if dark {
        cr.set_source_rgba(0.08, 0.16, 0.15, 0.95);
    } else {
        cr.set_source_rgba(0.96, 1., 0.99, 0.95);
    }
    cr.rectangle(x + inset, y + inset, side, side);
    let _ = cr.fill();
    color(cr, ink);
    cr.set_line_width((cell * 0.045).min(2.2));
    let period = side / (side / 6.).round().max(1.);
    let dash = period * 0.55;
    cr.set_dash(&[dash, period - dash], dash / 2.);
    cr.rectangle(x + inset, y + inset, side, side);
    let _ = cr.stroke();
    cr.set_dash(&[], 0.);
    draw_mark(
        cr,
        mark,
        x + cell / 2.,
        y + cell / 2.,
        side * if mark == Mark::Star { 1.35 } else { 1.8 },
        ink,
    );
    let _ = cr.restore();
}
fn draw_board(
    area: &gtk::DrawingArea,
    cr: &cairo::Context,
    width: i32,
    height: i32,
    board: &Board,
    session: &Session,
    source: Option<&gtk::gdk_pixbuf::Pixbuf>,
) {
    let n = board.size;
    let g = Geometry::new(width, height, n);
    let cell = g.cell;
    let frame = session.lesson_frame();
    let errors = frame.map_or_else(|| session.conflict_cells(board), |f| f.conflicts.clone());
    let suggestions: Vec<_> = if let Some(frame) = frame {
        frame
            .marks
            .iter()
            .enumerate()
            .filter(|&(i, m)| *m != board.marks[i])
            .map(|(i, &m)| (i, m))
            .collect()
    } else {
        session
            .targets()
            .iter()
            .copied()
            .filter(|(i, _)| !session.painted_cells().contains(i))
            .collect()
    };
    let dark_board = board
        .region_colors
        .first()
        .is_some_and(|rgb| rgb.iter().all(|&channel| channel < 128));
    let ink = if dark_board { (0.9, 0.9, 0.9) } else { INK };
    let teal = if dark_board { (0.3, 0.85, 0.75) } else { TEAL };
    let red = if dark_board { (1., 0.45, 0.45) } else { RED };
    let grid = if dark_board {
        (0.32, 0.36, 0.35)
    } else {
        (0.62, 0.66, 0.65)
    };
    let foreground = area.color();
    cr.set_source_rgba(
        foreground.red() as f64,
        foreground.green() as f64,
        foreground.blue() as f64,
        1.,
    );
    let layout = area.create_pango_layout(None);
    let mut font = area.pango_context().font_description().unwrap_or_default();
    font.set_size(9 * gtk::pango::SCALE);
    layout.set_font_description(Some(&font));
    for i in 0..n {
        layout.set_text(&Board::row_label(i).to_string());
        let (w, h) = layout.pixel_size();
        cr.move_to(
            g.left - w as f64 - 9.,
            g.top + (i as f64 + 0.5) * cell - h as f64 / 2.,
        );
        pangocairo::functions::show_layout(cr, &layout);
        layout.set_text(&(i + 1).to_string());
        let (w, h) = layout.pixel_size();
        cr.move_to(
            g.left + (i as f64 + 0.5) * cell - w as f64 / 2.,
            g.top - h as f64 - 6.,
        );
        pangocairo::functions::show_layout(cr, &layout);
    }
    if let Some(source) = source {
        // Both views use the same canvas, labels, and grid rectangle. Scale
        // each cropped image axis independently to align every cell boundary.
        let side = cell * n as f64;
        let _ = cr.save();
        cr.rectangle(g.left, g.top, side, side);
        cr.clip();
        cr.translate(g.left, g.top);
        cr.scale(side / source.width() as f64, side / source.height() as f64);
        cr.set_source_pixbuf(source, 0., 0.);
        cr.source().set_filter(cairo::Filter::Bilinear);
        let _ = cr.paint();
        let _ = cr.restore();
        color(cr, ink);
        cr.set_line_width(2.1);
        cr.rectangle(g.left, g.top, side, side);
        let _ = cr.stroke();
        return;
    }
    for i in 0..n * n {
        let (x, y) = g.origin(i);
        let region = board.regions[i];
        let rgb = board
            .region_colors
            .get(region)
            .copied()
            .unwrap_or(COLORS[region % COLORS.len()]);
        cr.set_source_rgb(
            rgb[0] as f64 / 255.,
            rgb[1] as f64 / 255.,
            rgb[2] as f64 / 255.,
        );
        cr.rectangle(x, y, cell, cell);
        let _ = cr.fill();
        if errors.contains(&i) {
            cr.set_source_rgba(0.85, 0.05, 0.08, 0.23);
            cr.rectangle(x, y, cell, cell);
            let _ = cr.fill();
        }
        let painted = session.painted_cells().contains(&i);
        if !suggestions.iter().any(|&(j, _)| j == i) {
            let mark = if painted { Mark::Cross } else { board.marks[i] };
            let mark_ink = if session.error_cells.contains(&i) && frame.is_none() {
                red
            } else {
                ink
            };
            draw_mark(cr, mark, x + cell / 2., y + cell / 2., cell, mark_ink);
        }
    }
    for i in 0..n * n {
        let (x, y) = g.origin(i);
        let (r, c) = (i / n, i % n);
        for (boundary, a, b) in [
            (
                c == 0 || board.regions[i] != board.regions[i - 1],
                (x, y),
                (x, y + cell),
            ),
            (
                r == 0 || board.regions[i] != board.regions[i - n],
                (x, y),
                (x + cell, y),
            ),
        ] {
            color(cr, if boundary { ink } else { grid });
            cr.set_line_width(if boundary { 2.1 } else { 0.6 });
            line(cr, a, b);
            let _ = cr.stroke();
        }
    }
    color(cr, ink);
    cr.set_line_width(2.1);
    cr.rectangle(g.left, g.top, cell * n as f64, cell * n as f64);
    let _ = cr.stroke();
    if errors.is_empty() {
        draw_hint_outline(cr, g, frame.map_or(&session.focus, |f| &f.focus));
    }
    // Outline the union of affected cells. A row/column count violation becomes
    // one continuous rectangle; a region follows its actual boundary.
    color(cr, red);
    cr.set_line_width(3.);
    for &i in &errors {
        let (x, y) = g.origin(i);
        let (r, c) = (i / n, i % n);
        if c == 0 || !errors.contains(&(i - 1)) {
            line(cr, (x + 1.5, y), (x + 1.5, y + cell));
        }
        if c + 1 == n || !errors.contains(&(i + 1)) {
            line(cr, (x + cell - 1.5, y), (x + cell - 1.5, y + cell));
        }
        if r == 0 || !errors.contains(&(i - n)) {
            line(cr, (x, y + 1.5), (x + cell, y + 1.5));
        }
        if r + 1 == n || !errors.contains(&(i + n)) {
            line(cr, (x, y + cell - 1.5), (x + cell, y + cell - 1.5));
        }
    }
    let _ = cr.stroke();
    // Suggestions are central badges, separated from group outlines and drawn
    // last so even a one-cell focus or contradiction cannot cover their dashes.
    for (i, mark) in suggestions {
        let (x, y) = g.origin(i);
        draw_suggestion(cr, mark, x, y, cell, teal, dark_board);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fast_drag_segments_cover_crossed_cells_and_clip_to_the_board() {
        let g = Geometry::new(740, 520, 10);
        let center = |i| {
            let (x, y) = g.origin(i);
            (x + g.cell / 2., y + g.cell / 2.)
        };
        assert_eq!(
            g.cells_along(center(20), center(29)),
            (20..30).collect::<Vec<_>>()
        );
        assert_eq!(
            g.cells_along(center(29), center(20)),
            (20..30).rev().collect::<Vec<_>>()
        );
        assert_eq!(
            g.cells_along(center(0), center(99)),
            (0..10).map(|i| i * 11).collect::<Vec<_>>()
        );
        assert_eq!(
            g.cells_along(
                (g.left - 100., center(20).1),
                (g.left + 1000., center(20).1)
            ),
            (20..30).collect::<Vec<_>>()
        );
        assert!(g.cells_along((0., 0.), (1000., 0.)).is_empty());
    }

    #[test]
    fn scaled_board_hit_testing_excludes_labels_and_edges() {
        for width in [400, 700, 1000] {
            let g = Geometry::new(width, 550, 10);
            for i in 0..100 {
                let (x, y) = g.origin(i);
                assert_eq!(g.cell_at(x + g.cell / 2., y + g.cell / 2.), Some(i));
            }
            assert_eq!(g.cell_at(g.left - 1., g.top), None);
            assert_eq!(g.cell_at(g.left, g.top - 1.), None);
            assert_eq!(g.cell_at(g.left + g.cell * 10. + 1., g.top), None);
        }
    }
}
