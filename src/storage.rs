use crate::model::Game;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

pub fn directory() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("starbattle")
}

pub fn load(dir: &Path) -> Result<Game, String> {
    let path = dir.join("game.json");
    let mut game: Game = if path.exists() {
        serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
    } else if dir.join("app.ron").exists() {
        // Read the previous egui save without altering it. Undo/redo are part of
        // Game, so the toolkit migration does not discard the player's history.
        let text = fs::read_to_string(dir.join("app.ron")).map_err(|e| e.to_string())?;
        let values: HashMap<String, String> = ron::from_str(&text).map_err(|e| e.to_string())?;
        match values.get("game-v1") {
            Some(value) => ron::from_str(value).map_err(|e| e.to_string())?,
            None => Game::default(),
        }
    } else {
        Game::default()
    };
    if !game.valid() {
        return Err("Saved board is invalid; the original save has been kept.".into());
    }
    game.refresh_automatic_crosses();
    Ok(game)
}

pub fn save(dir: &Path, game: &Game) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(game).map_err(|e| e.to_string())?;
    let temp = dir.join(format!("game-{}.tmp", std::process::id()));
    fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    fs::rename(temp, dir.join("game.json")).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Board, Mark};
    #[test]
    fn migrates_egui_save_with_history_and_keeps_original() {
        let dir = std::env::temp_dir().join(format!("starbattle-save-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut game = Game::default();
        game.load(Board::new(5, (0..25).map(|i| i / 5).collect(), vec![Mark::Empty; 25]).unwrap());
        game.change(|b| b.marks[0] = Mark::Cross);
        let original = ron::to_string(&HashMap::from([(
            "game-v1",
            ron::to_string(&game).unwrap(),
        )]))
        .unwrap();
        fs::write(dir.join("app.ron"), &original).unwrap();
        let mut migrated = load(&dir).unwrap();
        assert_eq!(migrated.board, game.board);
        migrated.undo();
        assert_eq!(migrated.board.as_ref().unwrap().marks[0], Mark::Empty);
        save(&dir, &migrated).unwrap();
        assert!(load(&dir).unwrap().can_redo());
        assert_eq!(fs::read_to_string(dir.join("app.ron")).unwrap(), original);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn colors_automatic_origins_and_disabled_preference_survive_restart() {
        let dir =
            std::env::temp_dir().join(format!("starbattle-auto-save-test-{}", std::process::id()));
        let mut game = Game::default();
        game.load(
            Board::new(
                10,
                (0..100).map(|i| i / 10).collect(),
                vec![Mark::Empty; 100],
            )
            .unwrap(),
        );
        let colors: Vec<_> = (0..10).map(|i| [100 + i * 10, 180, 220]).collect();
        game.board.as_mut().unwrap().region_colors = colors.clone();
        game.show_errors = false;
        game.play(&[(44, Mark::Star)]);
        save(&dir, &game).unwrap();
        let mut restored = load(&dir).unwrap();
        assert_eq!(restored.board, game.board);
        assert!(!restored.show_errors);
        restored.undo();
        assert_eq!(restored.board.as_ref().unwrap().region_colors, colors);
        restored.redo();
        assert_eq!(restored.board, game.board);
        restored.play(&[(44, Mark::Empty)]);
        assert_eq!(
            restored.board.as_ref().unwrap().marks,
            vec![Mark::Empty; 100]
        );
        restored.set_auto_crosses(false);
        save(&dir, &restored).unwrap();
        let mut restored = load(&dir).unwrap();
        assert!(!restored.auto_crosses);
        assert!(!restored.show_errors);
        assert_eq!(restored.board.as_ref().unwrap().region_colors, colors);
        restored
            .load(Board::new(5, (0..25).map(|i| i / 5).collect(), vec![Mark::Empty; 25]).unwrap());
        assert!(!restored.auto_crosses);
        assert!(!restored.show_errors);
        fs::remove_dir_all(dir).unwrap();
    }
}
