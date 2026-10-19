//! An independent exhaustive oracle for one-star boards. It enumerates column
//! permutations rather than using the solver's row masks or Board rule helpers.
mod common;
use star_battle_solver::{
    model::{Board, Mark},
    solver::{Advice, SearchResult, advise, solve},
};
use std::time::Duration;

fn arrangements(board: &Board) -> Vec<Vec<bool>> {
    fn visit(board: &Board, columns: &mut Vec<usize>, out: &mut Vec<Vec<bool>>) {
        let row = columns.len();
        if row == board.size {
            let mut regions = vec![0; board.size];
            let mut stars = vec![false; board.size * board.size];
            for (r, &c) in columns.iter().enumerate() {
                regions[board.regions[r * board.size + c]] += 1;
                stars[r * board.size + c] = true;
            }
            if regions.iter().all(|&count| count == 1) {
                out.push(stars);
            }
            return;
        }
        for col in 0..board.size {
            if columns.contains(&col) || columns.last().is_some_and(|&c| c.abs_diff(col) <= 1) {
                continue;
            }
            columns.push(col);
            visit(board, columns, out);
            columns.pop();
        }
    }
    let mut out = vec![];
    visit(board, &mut vec![], &mut out);
    out
}
fn compatible(marks: &[Mark], stars: &[bool]) -> bool {
    marks.iter().zip(stars).all(|(mark, &star)| match mark {
        Mark::Empty => true,
        Mark::Star => star,
        Mark::Cross => !star,
    })
}
fn random(seed: &mut u64) -> usize {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*seed >> 32) as usize
}

#[test]
fn exhaustive_oracle_checks_search_hints_and_corrections_on_generated_positions() {
    let site = common::expected("5x5-empty");
    let layouts = [
        site.regions.clone(),
        (0..25).map(|i| i / 5).collect(),
        (0..25).map(|i| i % 5).collect(),
        (0..25).map(|i| site.regions[(i % 5) * 5 + i / 5]).collect(),
        site.regions.iter().rev().copied().collect(),
    ];
    let mut seed = 0x5a17;
    let mut outcomes = [0; 5];
    for regions in layouts {
        let mut board = Board::new(5, regions, vec![Mark::Empty; 25]).unwrap();
        let all = arrangements(&board);
        assert!(!all.is_empty());
        for case in 0..100 {
            board.marks.fill(Mark::Empty);
            let answer = &all[random(&mut seed) % all.len()];
            for (i, &star) in answer.iter().enumerate() {
                if random(&mut seed) % 4 == 0 {
                    board.marks[i] = if star { Mark::Star } else { Mark::Cross };
                }
            }
            if case % 2 == 0 {
                for _ in 0..3 {
                    let i = random(&mut seed) % 25;
                    board.marks[i] = if random(&mut seed) % 2 == 0 {
                        Mark::Star
                    } else {
                        Mark::Cross
                    };
                }
            }
            let allowed: Vec<_> = all.iter().filter(|s| compatible(&board.marks, s)).collect();
            match solve(&board, Duration::from_secs(1)) {
                SearchResult::Solution(rows) => {
                    assert!(!allowed.is_empty());
                    let returned: Vec<_> =
                        (0..25).map(|i| rows[i / 5] & (1 << (i % 5)) != 0).collect();
                    assert!(allowed.contains(&&returned));
                }
                SearchResult::Impossible => assert!(allowed.is_empty()),
                SearchResult::Limit => panic!("5x5 search unexpectedly exceeded budget"),
            }
            match advise(&board, Duration::from_secs(1)) {
                Advice::Step(step) => {
                    outcomes[0] += 1;
                    assert!(!allowed.is_empty());
                    for (i, mark) in step.changes {
                        assert_eq!(board.marks[i], Mark::Empty);
                        assert!(allowed.iter().all(|s| s[i] == (mark == Mark::Star)));
                    }
                }
                Advice::Correction(step) => {
                    outcomes[1] += 1;
                    assert!(allowed.is_empty());
                    let mut assumptions = vec![Mark::Empty; 25];
                    for &(i, mark) in &step.premises {
                        assumptions[i] = mark;
                    }
                    let possible: Vec<_> =
                        all.iter().filter(|s| compatible(&assumptions, s)).collect();
                    assert!(!possible.is_empty());
                    for (i, mark) in step.changes {
                        assert_ne!(board.marks[i], Mark::Empty);
                        assert!(possible.iter().all(|s| s[i] == (mark == Mark::Star)));
                    }
                }
                Advice::Contradiction { cells, .. } => {
                    outcomes[2] += 1;
                    assert!(allowed.is_empty());
                    let mut core = vec![Mark::Empty; 25];
                    for i in cells {
                        core[i] = board.marks[i];
                    }
                    assert!(!all.iter().any(|s| compatible(&core, s)));
                }
                Advice::NoForcedMove => {
                    outcomes[3] += 1;
                    assert!(allowed.len() > 1);
                    for i in 0..25 {
                        if board.marks[i] == Mark::Empty {
                            assert!(allowed.iter().any(|s| s[i]));
                            assert!(allowed.iter().any(|s| !s[i]));
                        }
                    }
                }
                Advice::Solved => {
                    outcomes[4] += 1;
                    assert_eq!(allowed.len(), 1);
                    assert_eq!(board.marks.iter().filter(|&&m| m == Mark::Star).count(), 5);
                }
                Advice::Limit => panic!("5x5 advice unexpectedly exceeded budget"),
            }
        }
    }
    assert!(outcomes[..4].iter().all(|&n| n > 0), "{outcomes:?}");
    eprintln!("500 oracle positions; step/correction/conflict/ambiguous/solved: {outcomes:?}");
}
