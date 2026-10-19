# Learn Star Battle

**Learn Star Battle** is a native Rust app for Linux, using GTK 4 and libadwaita for GNOME windows, controls, and system light/dark styling. Paste a screenshot from **starbattlepuzzles.com**, review the recreated board, and play with dynamic hints and visual lessons.

## Run

Requires Rust 1.87 or newer, GTK 4.10+, and libadwaita 1.4+. On Ubuntu 24.04 / Debian with suitable library versions, install the build dependencies:

```sh
sudo apt-get install build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

The `gtk4` and `libadwaita` Rust crates wrap these native libraries; all application code is Rust. X11 and Wayland clipboard handling use GDK directly.

```sh
cargo run --release --locked
```

You can also open an image file on launch:

```sh
cargo run --release --locked -- /path/to/screenshot.png
```

Opening a screenshot while the app is running brings its existing window forward and opens import review. The current puzzle is kept until you choose **Start Playing**; **Cancel Import** returns to it. Finish or cancel an existing import before opening another image. Open one local screenshot at a time.

The build produced in this workspace is `target/release/star-battle-solver` and can be launched directly. The Rust toolchain and development packages are only needed to build. Running the binary requires the GTK 4 and libadwaita runtime libraries (`libgtk-4-1` and `libadwaita-1-0` on Ubuntu).

To install the built release, app icon, and GNOME launcher for your user:

```sh
sudo apt-get install desktop-file-utils
cargo build --release --locked
bash scripts/install.sh
```

Find **Learn Star Battle** in the applications overview. The launcher also accepts PNG and JPEG files through **Open With**. Installation defaults to `~/.local`; set an absolute `PREFIX` to stage it elsewhere. Rebuild and rerun the installer to update the installed copy. Installation does not change your saved puzzle or default image viewer.

The application ID is `io.github.elryno.LearnStarBattle`. When upgrading from the previous ID, the installer replaces the old launcher and icon. The executable name and saved-game location remain unchanged.

## Import and play

1. Dismiss the website's **Play** overlay. Take a screenshot containing the complete board, including its edges; surrounding page content is fine.
2. Focus Learn Star Battle and press **Ctrl-V**, or click **Paste Image**.
3. Compare the detected board with **Show Source**. The screenshot is cropped to the grid and scaled to the same position and size as the recreated board, with fixed row and column labels when you toggle or resize. The source view is read-only; switch back to correct a cell's mark. **Correct Regions** lets you choose a region and assign cells to it. Every region must remain connected before you can accept the import.
4. Click **Start Playing**.

The star count is inferred from the board size: 5×5 and 8×8 use one star; 10×10 uses two; 14×14 uses three. Other sizes are rejected.

Rows are labeled A, B, C… and columns 1, 2, 3…. Cell names use row then column: **I1** means row I, column 1.

Imported boards keep the screenshot's region colors, including the site's dark palette. Colors remain the same during play, undo/redo, and after restarting. Older saves without color information use the previous default colors; import their screenshot again to recover the original palette.

| Action | Input |
| --- | --- |
| Cycle empty → X → star → empty | Click a cell |
| Mark empty cells X, keeping existing X's and stars | Hold the left mouse button and drag across the board |

Hover over or click the **question-mark icon** beside the Main Menu in the header for mouse help. New Puzzle, Undo, and Redo are flat icon buttons at the left of the title bar. **Paste Image** appears only in the empty window, ready for the first board or after starting a new puzzle. Hover over Undo, Redo, or Paste Image to see their active shortcuts. The app uses GNOME's standard Ctrl-Z, Ctrl-Shift-Z, and Ctrl-V bindings, registered through GTK; tooltips read the current bindings. Cell input uses the mouse. The puzzle heading shows its size and star quota, with progress on the right.

The hamburger **Main Menu** also contains **How to Play** and **About Learn Star Battle**. The header follows system styling and retains its native divider. Advice sits beside the board in wide or short windows, and below it in tall, narrow windows. The board scales with the window, and the board and buttons stay visible in fixed positions while hints change. Typical explanations fit without scrolling; unusually long text scrolls only inside the advice panel. **Explain Hint** opens a visual lesson in the same panel. Longer explanations have Previous/Next controls and a page counter. Explain Hint, tutorial navigation, Apply Hint, and the solved-state New Puzzle button share a fixed footer, so actions never move as the text changes. The app fits a 1024×600 desktop.

Pasting is available only when there is no board, including through Ctrl-V. Use the **New Puzzle** icon at the left of the title bar to clear the current puzzle and its history, then paste another screenshot. A confirmation prevents accidental loss of an unfinished game; settings are kept. Once solved, a **New Puzzle** button appears in the fixed footer in the advice panel and returns directly to the paste screen without confirmation. Pasting is also disabled while an image is being read or reviewed.

**Auto-Place X's**, in the header's **Main Menu**, is on by default and remembered between launches. It marks empty cells around stars (including diagonals) and fills the rest of any row, column, or region whose star quota is met. On a 10×10 board that means two stars; other sizes use their own quota. Removing a star clears automatic X's that are no longer required. Shared X's stay while another star or a completed row, column, or region still requires them.

Manual X's are preserved. Imported X's are treated as manual because screenshots do not record their origin. Turning the setting off clears only automatic X's; turning it back on recalculates them from the current stars. Undo/Redo keep the setting you have selected, while restoring each move and its automatic marks together.

Dragging previews X's as you move and commits the whole stroke on release, as one undoable action. You can start on an existing symbol or drag over one without changing it. A cancelled drag leaves the board unchanged.

**Show Errors Automatically**, in the **Main Menu**, is on by default and remembered between launches and puzzles. Red shading and a continuous outline identify the selected rule violation: the **whole row, column, or region** for a count error, or the touching stars for an adjacency error. When there are multiple conflicts, select a message to inspect its corresponding cells. Turning the setting off hides live rule warnings and their highlights; requested hints and explanations still explain and highlight mistakes when needed. Completion always checks all counts and all eight neighboring cells; unused cells do not need X marks.

**Hint** is the single button below the board. It points to the cells and constraint to examine. **Explain Hint**, inside the advice panel, reveals a simple deduction directly or starts a visual lesson for a longer proof. **Previous** and **Next** walk through its deductions, updating the board to match the explanation. A bright orange outline identifies the supporting cells for the current page. Proposed symbols have smaller teal dashed boxes and light backing, drawn inside and above group outlines so both remain visible even for one cell.

During a **Trial · Puzzle Unchanged** page, the board is read-only. Simulated marks appear only in the lesson; they never enter your puzzle, save file, or undo history. Each page shows the current assumption or deduction, and a failed count or spacing rule is highlighted in red. **Back to Puzzle** leaves the lesson at any time. Navigating back through a search proof restores that branch's own snapshot.

At the conclusion, the actual board becomes playable again, with only proven suggestions visible. **Apply Hint** appears in the advice panel and places the remaining suggestions as one undoable move. You can also mark them manually: each completed suggestion disappears, and the hint clears when they are all done. Apply Hint remains available after partial manual progress. Applying a hint clears its explanation, highlights, and lesson.

Hints use your current stars and X's. Suggested marks include automatic X's when enabled. Editing, undoing, or changing the automatic-X setting discards any old tutorial snapshots; explaining the retained clue again analyzes the current board. The hint text, supporting highlights, and any revealed dashed targets stay visible through unrelated edits and partial progress. Each suggested mark loses its dashed box when placed; automatic-X proposals follow the current setting. The hint and its supporting highlights clear when all suggested marks are made, a supporting fact changes, or you start another puzzle. Hint is disabled while a clue or explanation is active, including after partial progress. It becomes available again when that advice clears. Selecting a rule-conflict message temporarily shows that conflict's highlights instead. Analysis results from an earlier position are discarded if you edit while analysis is running.

If the current marks make the puzzle unsolvable, **Hint** highlights a proven misplaced symbol in red and says how to change it. **Explain Hint** previews that correction before **Apply Hint** places it. Automatic X's are traced back to their stars. When no single correction can be proved, the hint highlights a conflicting group and explains that at least one of those marks must change; an unsolvable region layout is reported separately.

The current board and undo/redo history are saved locally to `$XDG_DATA_HOME/starbattle/game.json` (normally `~/.local/share/starbattle/game.json`). On first launch, the GTK version reads an existing `app.ron` save from the earlier interface, preserving the board and history without changing that original file. The display name is Learn Star Battle; the executable and storage location retain their existing names so saved games and launch commands remain compatible. Screenshot pixels are processed locally and are not sent to a service or saved with your game.

## Recognition and solver limits

- Recognition supports the site's standard colored light and dark themes, including existing stars and X's. Small dot notes are ignored and reported during review. It expects a straight, unobstructed screenshot; photographed, rotated, monochrome, or heavily compressed boards are outside the current scope. Cells must be at least about 15 pixels wide.
- The recognizer uses the site's current color palettes and regular grid. A site redesign may require updating it. Import review is intentional: correct misread regions or marks before playing.
- Assistance checks that the current marks admit a completion before adding deductions. Incorrect X's are constraints too. If the position has no completion, the app reports a contradiction instead of extending it.
- Assistance starts with immediate rules: neighbors of stars, completed counts, and exactly enough remaining cells. It then compares local deductions across all rows, columns, and regions, including matching the total stars in consecutive rows or columns to the regions they contain or occupy. These visible counts come before contradiction trials. Fewer possibilities or groups to consider take priority; equally simple deductions favor more immediate progress. If a trial is needed, it checks short contradictions before longer chains and lists every deduction and the specific conflict. Exhaustive search proofs are the last resort.
- A hint has an eight-second analysis budget; advanced proof transcripts also have a size limit. If it cannot prove a move in that time, it reports that limit without guessing. Ambiguous layouts are supported, but an arbitrary completion is never presented as a forced move.

## Development

The complete local check uses the same entry point as GitHub Actions:

```sh
sudo apt-get install build-essential pkg-config libgtk-4-dev libadwaita-1-dev libwayland-dev libwayland-bin xvfb xauth xdotool xclip imagemagick dbus-x11 ripgrep desktop-file-utils sway wl-clipboard wtype grim
bash scripts/check.sh
```

It checks formatting, strict Clippy, all Rust tests, native X11 and Wayland interaction, the release build, and a staged desktop installation. Missing tools or failed scenarios fail the check. `.github/workflows/check.yml` runs it on Ubuntu 24.04 with Rust 1.87 on pushes and pull requests, and preserves native screenshots and logs. A hosted run requires this repository to be uploaded to GitHub.

For just the Rust checks:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-features --locked
```

The modules separate board validation/history (`model`), screenshot recognition (`recognition`), constraint reasoning (`solver`), preview/edit transitions (`session`), save migration (`storage`), and the GTK interface (`ui`). Clipboard reads are asynchronous; recognition and solving run on worker threads.

Regression fixtures include real light/dark screenshots and visible cell labels from the site. Tests compare every detected region and mark, cover resized and full-page images, validate rules/history and save migration, reject contradictory positions, check preview/apply behavior, reproduce the reported hint and row-highlighting defects, and solve all four example sizes through repeated dynamic deductions.

Rules and size mapping: <https://www.starbattlepuzzles.com/how-to-play>.

Native interaction tests use an isolated X11 desktop and temporary saves. Install `xvfb`, `xdotool`, `xclip`, `imagemagick`, `dbus-x11`, and Python 3, then run:

```sh
cargo build --features capture --locked
bash scripts/test-native.sh
```

The runner covers native layout/teaching scenarios and real mouse, keyboard, clipboard, import correction, second-instance file opening, restart, and save-failure workflows. Results, logs, and screenshots go to `artifacts/native-review/`. Pass scenario names to run a subset (for example, `bash scripts/test-native.sh external teaching-narrow`). Ordinary builds exclude the automation module in `src/ui/capture.rs`.

`bash scripts/test-wayland.sh` uses a private headless Sway compositor at 100%, 125%, 150%, and 200% scale. It checks PNG clipboard transfer, source comparison, pointer placement and dragging, undo, and a visual hint lesson through actual Wayland input. Its small virtual-pointer helper is test-only. Results and compositor screenshots go to `artifacts/wayland-review/`. The runner records GTK's known popup-remapping fallback warning on Sway; other GTK warnings fail the check. These tests do not change the desktop's display settings or clipboard, and do not certify every GNOME/Mutter fractional-scale configuration.

For a manual check on your current Wayland desktop, copy `tests/fixtures/10x10-daily-page.png` as an image to the clipboard, then run `bash scripts/test-desktop-clipboard.sh`. It opens a temporary test window and compares all imported regions, colors, and marks with the known fixture. It reads the desktop clipboard and uses an isolated save directory.

The solver oracle independently enumerates every solution for generated 5×5 positions. A separate cell-based checker tests 48 deterministic partial positions across 10×10 and 14×14, verifies every suggested mark by proving its opposite impossible, checks four misplaced-symbol corrections, and validates the reported multi-step trial. It does not call the production constraint/search helpers. State-sequence tests compare 4,800 actions across all board sizes against an independent model. These supplement the example-puzzle, recognition, correction, and hint-lifetime regressions.
