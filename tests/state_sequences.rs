mod common;
use star_battle_solver::{
    model::{Board, Game, Mark},
    storage,
};

fn random(seed: &mut u64) -> usize {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*seed >> 32) as usize
}

// Reference marks are only explicit player input. Derive Xs by directly
// counting coordinates/regions, independently of Board::units/neighbors.
fn expected(board: &Board, manual: &[Mark], auto: bool) -> (Vec<Mark>, Vec<usize>) {
    let n = board.size;
    let k = match n {
        5 | 8 => 1,
        10 => 2,
        14 => 3,
        _ => unreachable!(),
    };
    let mut marks = manual.to_vec();
    let mut automatic = vec![];
    let stars: Vec<_> = (0..n * n).filter(|&j| manual[j] == Mark::Star).collect();
    for (i, mark) in marks.iter_mut().enumerate() {
        if !auto || *mark != Mark::Empty {
            continue;
        }
        if stars
            .iter()
            .any(|&j| (i / n).abs_diff(j / n) <= 1 && (i % n).abs_diff(j % n) <= 1)
            || stars.iter().filter(|&&j| i / n == j / n).count() == k
            || stars.iter().filter(|&&j| i % n == j % n).count() == k
            || stars
                .iter()
                .filter(|&&j| board.regions[i] == board.regions[j])
                .count()
                == k
        {
            *mark = Mark::Cross;
            automatic.push(i);
        }
    }
    (marks, automatic)
}

#[test]
fn mixed_play_settings_history_and_restart_match_an_independent_model() {
    let mut seed = 0x517a;
    let dir = std::env::temp_dir().join(format!("starbattle-sequences-{}", std::process::id()));
    for name in ["5x5-empty", "8x8-empty", "10x10-empty", "14x14-empty"] {
        let board = common::expected(name);
        let mut game = Game::default();
        game.load(board.clone());
        let mut manual = vec![Mark::Empty; board.marks.len()];
        let mut undo = Vec::new();
        let mut redo = Vec::new();
        for tick in 0..1200 {
            match random(&mut seed) % 10 {
                0 => game.set_auto_crosses(!game.auto_crosses),
                1 if !undo.is_empty() => {
                    redo.push(manual.clone());
                    manual = undo.pop().unwrap();
                    game.undo();
                }
                2 if !redo.is_empty() => {
                    undo.push(manual.clone());
                    manual = redo.pop().unwrap();
                    game.redo();
                }
                3 => {
                    storage::save(&dir, &game).unwrap();
                    game = storage::load(&dir).unwrap();
                }
                _ => {
                    let cell = random(&mut seed) % manual.len();
                    let mark = [Mark::Empty, Mark::Cross, Mark::Star][random(&mut seed) % 3];
                    let mut next = manual.clone();
                    next[cell] = mark;
                    if next != manual {
                        undo.push(manual);
                        if undo.len() > 500 {
                            undo.remove(0);
                        }
                        redo.clear();
                        manual = next;
                    }
                    game.play(&[(cell, mark)]);
                }
            }
            let (marks, automatic) = expected(&board, &manual, game.auto_crosses);
            let actual = game.board.as_ref().unwrap();
            assert_eq!(actual.marks, marks, "{name} at {tick}");
            assert_eq!(actual.automatic_crosses, automatic, "{name} at {tick}");
            assert_eq!(game.can_undo(), !undo.is_empty());
            assert_eq!(game.can_redo(), !redo.is_empty());
            actual.validate().unwrap();
        }
        assert!(game.valid());
    }
    std::fs::remove_dir_all(dir).unwrap();
}
