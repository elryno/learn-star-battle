//! A separate cell-based constraint checker, with no production rule/search helpers.
//! It branches on a complete unit's star combinations rather than row-mask domains.
mod common;
use star_battle_solver::{
    model::{Board, Mark},
    solver::{Advice, Step, advise},
};
use std::time::{Duration, Instant};

struct Oracle {
    units: Vec<Vec<usize>>,
    neighbors: Vec<Vec<usize>>,
    quota: usize,
}

impl Oracle {
    fn new(board: &Board) -> Self {
        let n = board.size;
        let mut units = Vec::new();
        for j in 0..n {
            units.push((0..n * n).filter(|i| i / n == j).collect());
            units.push((0..n * n).filter(|i| i % n == j).collect());
            units.push((0..n * n).filter(|&i| board.regions[i] == j).collect());
        }
        let neighbors = (0..n * n)
            .map(|i| {
                (0..n * n)
                    .filter(|&j| {
                        i != j && (i / n).abs_diff(j / n) <= 1 && (i % n).abs_diff(j % n) <= 1
                    })
                    .collect()
            })
            .collect();
        Self {
            units,
            neighbors,
            quota: if n == 10 {
                2
            } else {
                assert_eq!(n, 14);
                3
            },
        }
    }

    fn solve(&self, marks: &[Mark]) -> Option<Vec<Mark>> {
        self.search(marks.to_vec(), Instant::now() + Duration::from_secs(15))
    }

    fn search(&self, mut marks: Vec<Mark>, deadline: Instant) -> Option<Vec<Mark>> {
        assert!(
            Instant::now() < deadline,
            "Independent checker exceeded its budget; no verdict claimed"
        );
        loop {
            let before = marks.clone();
            for i in 0..marks.len() {
                if marks[i] != Mark::Star {
                    continue;
                }
                for &j in &self.neighbors[i] {
                    if marks[j] == Mark::Star {
                        return None;
                    }
                    marks[j] = Mark::Cross;
                }
            }
            for unit in &self.units {
                let stars = unit.iter().filter(|&&i| marks[i] == Mark::Star).count();
                let empty: Vec<_> = unit
                    .iter()
                    .copied()
                    .filter(|&i| marks[i] == Mark::Empty)
                    .collect();
                if stars > self.quota || stars + empty.len() < self.quota {
                    return None;
                }
                if stars == self.quota || stars + empty.len() == self.quota {
                    for i in empty {
                        marks[i] = if stars == self.quota {
                            Mark::Cross
                        } else {
                            Mark::Star
                        };
                    }
                }
            }
            if marks == before {
                break;
            }
        }
        let choices = self
            .units
            .iter()
            .filter_map(|unit| {
                let need = self.quota - unit.iter().filter(|&&i| marks[i] == Mark::Star).count();
                let empty: Vec<_> = unit
                    .iter()
                    .copied()
                    .filter(|&i| marks[i] == Mark::Empty)
                    .collect();
                if empty.is_empty() {
                    return None;
                }
                let ways = (0..need).fold(1_usize, |a, k| a * (empty.len() - k) / (k + 1));
                Some((ways, need, empty))
            })
            .min_by_key(|(ways, _, _)| *ways);
        let Some((_, need, empty)) = choices else {
            return Some(marks);
        };
        self.choose(&marks, &empty, need, &mut Vec::new(), 0, deadline)
    }

    fn choose(
        &self,
        marks: &[Mark],
        empty: &[usize],
        need: usize,
        stars: &mut Vec<usize>,
        start: usize,
        deadline: Instant,
    ) -> Option<Vec<Mark>> {
        if stars.len() == need {
            let mut trial = marks.to_vec();
            for &i in empty {
                trial[i] = Mark::Cross;
            }
            for &i in stars.iter() {
                trial[i] = Mark::Star;
            }
            return self.search(trial, deadline);
        }
        for j in start..=empty.len() - (need - stars.len()) {
            let i = empty[j];
            if stars.iter().any(|s| self.neighbors[i].contains(s)) {
                continue;
            }
            stars.push(i);
            let answer = self.choose(marks, empty, need, stars, j + 1, deadline);
            stars.pop();
            if answer.is_some() {
                return answer;
            }
        }
        None
    }

    fn check_step(&self, assumptions: &[Mark], step: &Step) {
        assert!(
            self.solve(assumptions).is_some(),
            "A deduction needs satisfiable premises"
        );
        assert!(!step.changes.is_empty());
        for &(i, mark) in &step.changes {
            assert_ne!(mark, Mark::Empty);
            let mut opposite = assumptions.to_vec();
            opposite[i] = if mark == Mark::Star {
                Mark::Cross
            } else {
                Mark::Star
            };
            assert!(
                self.solve(&opposite).is_none(),
                "Unproven suggestion at cell {i}: {step:?}"
            );
        }
    }
}

#[test]
fn two_and_three_star_hints_and_repairs_have_independent_proofs() {
    let mut counts = [0; 2];
    for (which, name) in ["10x10-empty", "14x14-empty"].into_iter().enumerate() {
        let mut board = common::expected(name);
        let oracle = Oracle::new(&board);
        let solution = oracle.solve(&board.marks).expect("independent solution");
        let mut seed = 0x7a3b_u64;
        for case in 0..24 {
            board.marks = solution
                .iter()
                .map(|&m| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    if (seed >> 32) % 100 < [25, 50, 75][case % 3] {
                        m
                    } else {
                        Mark::Empty
                    }
                })
                .collect();
            let Advice::Step(step) = advise(&board, Duration::from_secs(8)) else {
                panic!("{name} case {case}: expected deduction");
            };
            oracle.check_step(&board.marks, &step);
            counts[which] += step.changes.len();
        }
        // A wrong X and a wrong star on each unique site layout.
        // Check the correction using only its advertised supporting facts.
        for wrong in [Mark::Star, Mark::Cross] {
            board.marks.fill(Mark::Empty);
            let i = solution.iter().position(|&m| m != wrong).unwrap();
            board.marks[i] = wrong;
            assert!(oracle.solve(&board.marks).is_none());
            let result = advise(&board, Duration::from_secs(8));
            let Advice::Correction(step) = result else {
                panic!("{name}, {wrong:?}: expected correction, got {result:?}");
            };
            let mut premises = vec![Mark::Empty; board.marks.len()];
            for &(i, mark) in &step.premises {
                premises[i] = mark;
            }
            oracle.check_step(&premises, &step);
        }
    }
    eprintln!(
        "48 multi-star positions, independently proven marks by size: {counts:?}; 4 corrections"
    );
}

#[test]
fn the_reported_multistep_trial_has_an_independently_proven_conclusion() {
    let board: Board =
        serde_json::from_str(include_str!("fixtures/10x10-daily-page.json")).unwrap();
    let Advice::Step(step) = advise(&board, Duration::from_secs(8)) else {
        panic!("expected tutorial");
    };
    assert!(
        !step.frames.is_empty(),
        "This regression must exercise a multi-step explanation"
    );
    Oracle::new(&board).check_step(&board.marks, &step);
}
