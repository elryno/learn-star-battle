mod common;
use star_battle_solver::{
    model::{Game, Mark},
    storage,
};
use std::{fs, path::PathBuf};

fn directory(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("starbattle-{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}
#[test]
fn corrupt_current_save_is_reported_without_overwriting_or_falling_back() {
    let dir = directory("corrupt-save");
    let original = b"{truncated";
    fs::write(dir.join("game.json"), original).unwrap();
    fs::write(dir.join("app.ron"), "{}").unwrap();
    assert!(storage::load(&dir).is_err());
    assert_eq!(fs::read(dir.join("game.json")).unwrap(), original);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn invalid_current_board_and_history_are_rejected_without_panics() {
    let dir = directory("invalid-save");
    let mut game = Game::default();
    game.load(common::expected("5x5-empty"));
    game.play(&[(0, Mark::Cross)]);
    let original = serde_json::to_value(game).unwrap();
    for field in ["board", "undo"] {
        for invalid in [
            "size",
            "marks",
            "regions",
            "automatic_crosses",
            "region_colors",
        ] {
            let mut json = original.clone();
            let board = if field == "undo" {
                &mut json[field][0]
            } else {
                &mut json[field]
            };
            board[invalid] = match invalid {
                "size" => serde_json::json!(usize::MAX),
                "automatic_crosses" => serde_json::json!([1000]),
                "region_colors" => serde_json::json!([[255, 0, 0]]),
                _ => serde_json::json!([]),
            };
            let bytes = serde_json::to_vec(&json).unwrap();
            fs::write(dir.join("game.json"), &bytes).unwrap();
            assert!(storage::load(&dir).is_err(), "{field}/{invalid}");
            assert_eq!(fs::read(dir.join("game.json")).unwrap(), bytes);
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn failed_temporary_write_keeps_the_previous_save() {
    let dir = directory("failed-save");
    let mut game = Game::default();
    game.load(common::expected("5x5-empty"));
    storage::save(&dir, &game).unwrap();
    let original = fs::read(dir.join("game.json")).unwrap();
    fs::create_dir(dir.join(format!("game-{}.tmp", std::process::id()))).unwrap();
    game.play(&[(0, Mark::Star)]);
    assert!(storage::save(&dir, &game).is_err());
    assert_eq!(fs::read(dir.join("game.json")).unwrap(), original);
    fs::remove_dir_all(dir).unwrap();
}
