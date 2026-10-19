use crate::model::{Board, Mark};
use std::{
    cell::RefCell,
    cmp::Reverse,
    time::{Duration, Instant},
};

/// A read-only snapshot of a hypothetical deduction, never part of the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReasoningFrame {
    pub text: String,
    pub marks: Vec<Mark>,
    pub focus: Vec<usize>,
    pub conflicts: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Step {
    pub clue: String,
    pub explanation: String,
    pub focus: Vec<usize>,
    pub changes: Vec<(usize, Mark)>,
    /// Existing marks needed by this explanation, separate from its highlight.
    pub premises: Vec<(usize, Mark)>,
    pub frames: Vec<ReasoningFrame>,
}

fn addresses(board: &Board, cells: &[usize]) -> String {
    cells
        .iter()
        .map(|&i| board.address(i))
        .collect::<Vec<_>>()
        .join(", ")
}

fn counted(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

fn star_range(low: usize, high: usize) -> String {
    if low == high {
        counted(low, "star")
    } else {
        format!("between {low} and {high} stars")
    }
}

fn premises(board: &Board, cells: &[usize]) -> Vec<(usize, Mark)> {
    board
        .marks
        .iter()
        .enumerate()
        .filter_map(|(i, &mark)| {
            (mark != Mark::Empty
                && (cells.contains(&i)
                    || (mark == Mark::Star && cells.iter().any(|&j| touching(board, i, j)))))
            .then_some((i, mark))
        })
        .collect()
}

fn action(board: &Board, changes: &[(usize, Mark)]) -> String {
    [Mark::Star, Mark::Cross]
        .into_iter()
        .filter_map(|mark| {
            let cells: Vec<_> = changes
                .iter()
                .filter(|(_, m)| *m == mark)
                .map(|&(i, _)| i)
                .collect();
            (!cells.is_empty()).then(|| match mark {
                Mark::Star => format!(
                    "Place {} at {}.",
                    if cells.len() == 1 { "a star" } else { "stars" },
                    addresses(board, &cells)
                ),
                _ => format!(
                    "Mark {} with {}.",
                    addresses(board, &cells),
                    if cells.len() == 1 { "an X" } else { "Xs" }
                ),
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug)]
pub enum Advice {
    Step(Step),
    Correction(Step),
    Contradiction { message: String, cells: Vec<usize> },
    Solved,
    NoForcedMove,
    Limit,
}

#[derive(Debug)]
pub enum SearchResult {
    Solution(Vec<u16>),
    Impossible,
    Limit,
}

struct Search<'a> {
    board: &'a Board,
    deadline: Instant,
    nodes: usize,
    // Each column and each region, represented as a mask per row.
    units: Vec<Vec<u16>>,
    trace: Option<RefCell<Vec<ReasoningFrame>>>,
}

impl<'a> Search<'a> {
    fn new(board: &'a Board, deadline: Instant) -> Self {
        let n = board.size;
        let mut units = vec![vec![0; n]; n * 2];
        for r in 0..n {
            for c in 0..n {
                units[c][r] |= 1 << c;
                units[n + board.regions[r * n + c]][r] |= 1 << c;
            }
        }
        Self {
            board,
            deadline,
            nodes: 0,
            units,
            trace: None,
        }
    }
    fn expired(&self) -> bool {
        self.nodes > 250_000
            || Instant::now() >= self.deadline
            || self.trace.as_ref().is_some_and(|t| t.borrow().len() > 2000)
    }
    fn record(
        &self,
        domains: &[Vec<u16>],
        focus: Vec<usize>,
        conflict: bool,
        text: impl FnOnce() -> String,
    ) {
        if let Some(trace) = &self.trace {
            let mut marks = self.board.marks.clone();
            for (r, choices) in domains.iter().enumerate() {
                if choices.is_empty() {
                    continue;
                }
                for c in 0..self.board.size {
                    let bit = 1 << c;
                    if choices.iter().all(|m| m & bit != 0) {
                        marks[r * self.board.size + c] = Mark::Star;
                    } else if choices.iter().all(|m| m & bit == 0) {
                        marks[r * self.board.size + c] = Mark::Cross;
                    }
                }
            }
            trace.borrow_mut().push(ReasoningFrame {
                text: text(),
                marks,
                conflicts: if conflict { focus.clone() } else { vec![] },
                focus,
            });
        }
    }
    fn row_cells(&self, row: usize) -> Vec<usize> {
        (row * self.board.size..(row + 1) * self.board.size).collect()
    }
    fn unit_cells(&self, masks: &[u16]) -> Vec<usize> {
        (0..self.board.marks.len())
            .filter(|&i| masks[i / self.board.size] & (1 << (i % self.board.size)) != 0)
            .collect()
    }
    fn describe(&self, row: usize, patterns: &[u16]) -> String {
        if patterns.is_empty() {
            return "none".into();
        }
        patterns
            .iter()
            .map(|&mask| {
                let cells: Vec<_> = (0..self.board.size)
                    .filter(|&c| mask & (1 << c) != 0)
                    .map(|c| row * self.board.size + c)
                    .collect();
                format!("({})", addresses(self.board, &cells))
            })
            .collect::<Vec<_>>()
            .join(" or ")
    }
    fn domains(&self, assumption: Option<(usize, Mark)>) -> Vec<Vec<u16>> {
        let n = self.board.size;
        let patterns: Vec<u16> = (0..1u16 << n)
            .filter(|&m| m.count_ones() == self.board.required() as u32 && m & (m << 1) == 0)
            .collect();
        (0..n)
            .map(|r| {
                let mut stars = 0;
                let mut crosses = 0;
                for c in 0..n {
                    let i = r * n + c;
                    let mark = assumption
                        .filter(|&(j, _)| j == i)
                        .map(|(_, m)| m)
                        .unwrap_or(self.board.marks[i]);
                    match mark {
                        Mark::Star => stars |= 1 << c,
                        Mark::Cross => crosses |= 1 << c,
                        Mark::Empty => {}
                    }
                }
                patterns
                    .iter()
                    .copied()
                    .filter(|&m| m & stars == stars && m & crosses == 0)
                    .collect()
            })
            .collect()
    }
    fn propagate(&self, domains: &mut [Vec<u16>]) -> Result<bool, ()> {
        let n = self.board.size;
        loop {
            if self.expired() {
                return Err(());
            }
            if let Some(row) = domains.iter().position(Vec::is_empty) {
                self.record(domains, self.row_cells(row), true, || {
                    format!(
                        "Row {} has no possible star placement left.",
                        Board::row_label(row)
                    )
                });
                return Ok(false);
            }
            let before: usize = domains.iter().map(Vec::len).sum();
            for r in 0..n {
                for neighbor in [r.checked_sub(1), (r + 1 < n).then_some(r + 1)]
                    .into_iter()
                    .flatten()
                {
                    let other = domains[neighbor].clone();
                    let count = domains[r].len();
                    domains[r].retain(|&m| {
                        let blocked = m | (m << 1) | (m >> 1);
                        other.iter().any(|&p| p & blocked == 0)
                    });
                    if domains[r].len() != count {
                        self.record(domains, [self.row_cells(r), self.row_cells(neighbor)].concat(), false, ||format!("Stars in row {} cannot touch stars in row {}. The possibilities for row {} are {}. This leaves the following choices for row {}: {}.",
                            Board::row_label(r),Board::row_label(neighbor),Board::row_label(neighbor),self.describe(neighbor,&other),Board::row_label(r),self.describe(r,&domains[r])));
                    }
                }
            }
            if let Some(row) = domains.iter().position(Vec::is_empty) {
                self.record(domains, self.row_cells(row), true, || {
                    format!(
                        "Row {} has no possible star placement left.",
                        Board::row_label(row)
                    )
                });
                return Ok(false);
            }
            for (unit, masks) in self.units.iter().enumerate() {
                let bounds: Vec<(usize, usize)> = domains
                    .iter()
                    .enumerate()
                    .map(|(r, ds)| {
                        let counts = ds.iter().map(|&d| (d & masks[r]).count_ones() as usize);
                        (counts.clone().min().unwrap_or(0), counts.max().unwrap_or(0))
                    })
                    .collect();
                let low: usize = bounds.iter().map(|b| b.0).sum();
                let high: usize = bounds.iter().map(|b| b.1).sum();
                let k = self.board.required();
                let name = || {
                    if unit < n {
                        format!("Column {}", unit + 1)
                    } else {
                        format!("Region {}", unit - n + 1)
                    }
                };
                if low > k || high < k {
                    self.record(domains, self.unit_cells(masks), true, || {
                        format!(
                            "{} needs {}, but the remaining placements allow {}.",
                            name(),
                            counted(k, "star"),
                            star_range(low, high)
                        )
                    });
                    return Ok(false);
                }
                for r in 0..n {
                    let count = domains[r].len();
                    domains[r].retain(|&d| {
                        let count = (d & masks[r]).count_ones() as usize;
                        low - bounds[r].0 + count <= k && high - bounds[r].1 + count >= k
                    });
                    if domains[r].len() != count {
                        self.record(domains, self.unit_cells(masks), false, ||format!("{} needs {}. The other rows account for {} here, so row {} must contribute {}. Its remaining placements are: {}.",
                            name(),counted(k, "star"), star_range(low-bounds[r].0, high-bounds[r].1), Board::row_label(r), star_range(k.saturating_sub(high-bounds[r].1), k-(low-bounds[r].0)),self.describe(r,&domains[r])));
                    }
                }
            }
            if let Some(row) = domains.iter().position(Vec::is_empty) {
                self.record(domains, self.row_cells(row), true, || {
                    format!(
                        "Row {} has no possible star placement left.",
                        Board::row_label(row)
                    )
                });
                return Ok(false);
            }
            if before == domains.iter().map(Vec::len).sum::<usize>() {
                return Ok(true);
            }
        }
    }
    fn run(&mut self, mut domains: Vec<Vec<u16>>) -> SearchResult {
        self.run_branch(&mut domains)
    }
    fn run_branch(&mut self, domains: &mut [Vec<u16>]) -> SearchResult {
        self.nodes += 1;
        match self.propagate(domains) {
            Err(()) => return SearchResult::Limit,
            Ok(false) => return SearchResult::Impossible,
            Ok(true) => {}
        }
        let row = (0..domains.len())
            .filter(|&r| domains[r].len() > 1)
            .min_by_key(|&r| domains[r].len());
        let Some(row) = row else {
            return SearchResult::Solution(domains.iter().map(|d| d[0]).collect());
        };
        for mask in domains[row].clone() {
            let mut branch = domains.to_vec();
            branch[row] = vec![mask];
            self.record(&branch, self.row_cells(row), false, || {
                format!(
                    "Consider this possibility for row {}: {}.",
                    Board::row_label(row),
                    self.describe(row, &[mask])
                )
            });
            match self.run_branch(&mut branch) {
                SearchResult::Impossible => {
                    self.record(domains, self.row_cells(row), false, || {
                        "That possibility breaks a rule. Return to the previous position and consider the remaining possibilities.".into()
                    });
                }
                result => return result,
            }
        }
        self.record(domains, self.row_cells(row), true, || {
            format!(
                "None of the possible placements in row {} works with our earlier assumptions.",
                Board::row_label(row)
            )
        });
        SearchResult::Impossible
    }
}

pub fn solve(board: &Board, budget: Duration) -> SearchResult {
    let mut search = Search::new(board, Instant::now() + budget);
    let domains = search.domains(None);
    search.run(domains)
}

fn search_until(board: &Board, deadline: Instant) -> SearchResult {
    let mut search = Search::new(board, deadline);
    let domains = search.domains(None);
    search.run(domains)
}

fn correction(board: &Board, cell: usize, reason: &str, premises: Vec<(usize, Mark)>) -> Advice {
    let (from, to, mark) = match board.marks[cell] {
        Mark::Cross => ("X", "a star", Mark::Star),
        Mark::Star => ("star", "an X", Mark::Cross),
        Mark::Empty => unreachable!("only existing symbols need correction"),
    };
    let text = format!(
        "Change the {from} at {} to {to}. {reason}",
        board.address(cell)
    );
    Advice::Correction(Step {
        clue: text.clone(),
        explanation: text,
        focus: vec![cell],
        changes: vec![(cell, mark)],
        premises,
        frames: vec![],
    })
}

/// Find a contradictory set of player marks, never blaming X's derived from stars.
/// A symbol is called wrong only after proving it cannot be kept under the stated
/// assumptions. Finding one completion is not evidence that other marks are wrong.
fn diagnose(board: &Board, deadline: Instant) -> Advice {
    let unresolved = || {
        Advice::Contradiction {
        message: "The puzzle cannot be solved with these marks, but finding a specific correction is taking too long. Try undoing a recent move, then ask for another hint.".into(),
        cells: vec![],
    }
    };
    let mut explicit = board.clone();
    explicit.refresh_automatic_crosses(false);
    let mut bare = explicit.clone();
    bare.marks.fill(Mark::Empty);
    let solution = match search_until(&bare, deadline) {
        SearchResult::Impossible => return Advice::Contradiction {
            message: "This region layout has no solution, even with all marks removed. Check that the imported regions match the screenshot.".into(),
            cells: vec![],
        },
        SearchResult::Limit => return unresolved(),
        SearchResult::Solution(solution) => solution,
    };
    let mut cells: Vec<_> = (0..explicit.marks.len())
        .filter(|&i| explicit.marks[i] != Mark::Empty)
        .collect();
    // Remove marks compatible with a known completion first; each removal still
    // requires proof that the remaining marks are contradictory.
    cells.sort_by_key(|&i| {
        (solution[i / board.size] & (1 << (i % board.size)) != 0)
            != (explicit.marks[i] == Mark::Star)
    });
    let mut core = explicit.clone();
    let mut limited = false;
    for i in cells {
        let mark = core.marks[i];
        core.marks[i] = Mark::Empty;
        match search_until(&core, deadline) {
            SearchResult::Impossible => {}
            SearchResult::Solution(_) => core.marks[i] = mark,
            SearchResult::Limit => {
                core.marks[i] = mark;
                limited = true;
                break;
            }
        }
    }
    let cells: Vec<_> = (0..core.marks.len())
        .filter(|&i| core.marks[i] != Mark::Empty)
        .collect();
    if let [cell] = cells.as_slice() {
        return correction(
            board,
            *cell,
            "This cell must have that mark in every solution, regardless of your other marks.",
            vec![],
        );
    }
    if !limited {
        // On a layout with multiple solutions, a mark may be wrong only in
        // combination with others. Offer a repair only if it really restores a
        // completion while preserving every other player mark.
        for &i in &cells {
            let mark = explicit.marks[i];
            explicit.marks[i] = Mark::Empty;
            let result = search_until(&explicit, deadline);
            explicit.marks[i] = mark;
            match result {
                SearchResult::Solution(_) => {
                    return correction(
                        board,
                        i,
                        "Keeping your other placed marks, this is the only choice that allows the puzzle to be solved.",
                        explicit
                            .marks
                            .iter()
                            .enumerate()
                            .filter(|&(j, &m)| j != i && m != Mark::Empty)
                            .map(|(j, &m)| (j, m))
                            .collect(),
                    );
                }
                SearchResult::Impossible => {}
                SearchResult::Limit => {
                    limited = true;
                    break;
                }
            }
        }
    }
    let symbols = cells
        .iter()
        .map(|&i| {
            format!(
                "{} at {}",
                if board.marks[i] == Mark::Star {
                    "star"
                } else {
                    "X"
                },
                board.address(i)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    Advice::Contradiction {
        message: format!(
            "These highlighted marks cannot all be correct: {symbols}. Remove or change at least one of them, then ask for another hint. {}",
            if limited {
                "Finding a specific correction is taking too long."
            } else {
                "More than one mark needs to change before the puzzle can be solved."
            }
        ),
        cells,
    }
}

fn simple_step(board: &Board) -> Option<Step> {
    for (i, mark) in board.marks.iter().enumerate() {
        if *mark != Mark::Star {
            continue;
        }
        let changes: Vec<_> = board
            .neighbors(i)
            .into_iter()
            .filter(|&j| board.marks[j] == Mark::Empty)
            .map(|j| (j, Mark::Cross))
            .collect();
        if !changes.is_empty() {
            return Some(Step {
                clue: format!(
                    "Stars cannot touch, even diagonally. Which empty cells are next to the star at {}?",
                    board.address(i)
                ),
                explanation: format!(
                    "Stars cannot touch, even diagonally. The empty cells next to {} cannot contain stars. {}",
                    board.address(i),
                    action(board, &changes)
                ),
                focus: std::iter::once(i)
                    .chain(changes.iter().map(|&(j, _)| j))
                    .collect(),
                changes,
                premises: vec![(i, Mark::Star)],
                frames: vec![],
            });
        }
    }
    for (name, cells) in board.units() {
        let stars = cells
            .iter()
            .filter(|&&i| board.marks[i] == Mark::Star)
            .count();
        let empty: Vec<_> = cells
            .iter()
            .copied()
            .filter(|&i| board.marks[i] == Mark::Empty)
            .collect();
        if empty.is_empty() {
            continue;
        }
        let mark = if stars == board.required() {
            Some(Mark::Cross)
        } else if stars + empty.len() == board.required() {
            Some(Mark::Star)
        } else {
            None
        };
        if let Some(mark) = mark {
            let explanation = if mark == Mark::Cross {
                format!(
                    "{name} already has its {}. The remaining cells cannot contain stars.",
                    counted(board.required(), "star")
                )
            } else {
                format!(
                    "{name} still needs {} and has exactly {} available. {}",
                    counted(board.required() - stars, "star"),
                    counted(empty.len(), "cell"),
                    if empty.len() == 1 {
                        "That cell must contain a star."
                    } else {
                        "Each of those cells must contain a star."
                    }
                )
            };
            return Some(Step {
                clue: format!(
                    "{name} {} {}",
                    if mark == Mark::Cross {
                        format!("already has its {}.", counted(stars, "star"))
                    } else {
                        format!(
                            "needs {} more {} and has {} left.",
                            board.required() - stars,
                            if board.required() - stars == 1 {
                                "star"
                            } else {
                                "stars"
                            },
                            counted(empty.len(), "empty cell")
                        )
                    },
                    if mark == Mark::Cross {
                        "Can any empty cell hold another star?"
                    } else if empty.len() == 1 {
                        "Where must the last star go?"
                    } else {
                        "Where must the remaining stars go?"
                    }
                ),
                explanation: format!(
                    "{explanation} {}",
                    action(board, &empty.iter().map(|&i| (i, mark)).collect::<Vec<_>>())
                ),
                premises: cells
                    .iter()
                    .filter_map(|&i| (board.marks[i] != Mark::Empty).then_some((i, board.marks[i])))
                    .collect(),
                focus: cells,
                changes: empty.into_iter().map(|i| (i, mark)).collect(),
                frames: vec![],
            });
        }
    }
    None
}

fn touching(board: &Board, a: usize, b: usize) -> bool {
    (a / board.size).abs_diff(b / board.size) <= 1 && (a % board.size).abs_diff(b % board.size) <= 1
}

/// Enumerate only the local count and spacing rule for one unit. Unlike the
/// search domains, these possibilities have no hidden propagation history.
fn placements(board: &Board, cells: &[usize], deadline: Instant) -> Option<Vec<Vec<usize>>> {
    let fixed: Vec<_> = cells
        .iter()
        .copied()
        .filter(|&i| board.marks[i] == Mark::Star)
        .collect();
    let needed = board.required().checked_sub(fixed.len())?;
    let candidates: Vec<_> = cells
        .iter()
        .copied()
        .filter(|&i| {
            board.marks[i] == Mark::Empty
                && board
                    .neighbors(i)
                    .iter()
                    .all(|&j| board.marks[j] != Mark::Star)
        })
        .collect();
    struct Enumeration<'a> {
        board: &'a Board,
        candidates: &'a [usize],
        deadline: Instant,
        visited: usize,
        result: Vec<Vec<usize>>,
    }
    impl Enumeration<'_> {
        fn visit(&mut self, start: usize, needed: usize, chosen: &mut Vec<usize>) -> Option<()> {
            self.visited += 1;
            if self.visited > 20_000 || Instant::now() >= self.deadline {
                return None;
            }
            if needed == 0 {
                self.result.push(chosen.clone());
                return Some(());
            }
            for index in start..self.candidates.len() {
                if self.candidates.len() - index < needed {
                    break;
                }
                let cell = self.candidates[index];
                if chosen
                    .iter()
                    .all(|&other| !touching(self.board, cell, other))
                {
                    chosen.push(cell);
                    self.visit(index + 1, needed - 1, chosen)?;
                    chosen.pop();
                }
            }
            Some(())
        }
    }
    let mut enumeration = Enumeration {
        board,
        candidates: &candidates,
        deadline,
        visited: 0,
        result: vec![],
    };
    enumeration.visit(0, needed, &mut vec![])?;
    Some(enumeration.result)
}

/// Equal totals in a band of rows/columns and a set of regions leave no stars
/// for the cells outside their overlap. This uses only visible boundaries and
/// quotas, not a hypothetical placement or a completed solution.
fn group_count_step(board: &Board, deadline: Instant) -> Option<(usize, Step)> {
    let n = board.size;
    let regions: Vec<Vec<usize>> = (0..n)
        .map(|region| (0..n * n).filter(|&i| board.regions[i] == region).collect())
        .collect();
    for count in 1..n {
        let mut best: Option<Step> = None;
        for columns in [false, true] {
            for start in 0..=n - count {
                if Instant::now() >= deadline {
                    return best.map(|step| (count, step));
                }
                let in_band: Vec<_> = (0..n * n)
                    .map(|i| {
                        let line = if columns { i % n } else { i / n };
                        (start..start + count).contains(&line)
                    })
                    .collect();
                for regions_inside in [true, false] {
                    let selected: Vec<_> = regions
                        .iter()
                        .enumerate()
                        .filter(|(_, cells)| {
                            if regions_inside {
                                cells.iter().all(|&i| in_band[i])
                            } else {
                                cells.iter().any(|&i| in_band[i])
                            }
                        })
                        .map(|(r, _)| r)
                        .collect();
                    if selected.len() != count {
                        continue;
                    }
                    let changes: Vec<_> = (0..n * n)
                        .filter(|&i| {
                            let in_regions = selected.contains(&board.regions[i]);
                            board.marks[i] == Mark::Empty
                                && if regions_inside {
                                    in_band[i] && !in_regions
                                } else {
                                    in_regions && !in_band[i]
                                }
                        })
                        .map(|i| (i, Mark::Cross))
                        .collect();
                    if changes.is_empty()
                        || best
                            .as_ref()
                            .is_some_and(|step| step.changes.len() >= changes.len())
                    {
                        continue;
                    }
                    let line_label = |i: usize| {
                        if columns {
                            (i + 1).to_string()
                        } else {
                            Board::row_label(i).to_string()
                        }
                    };
                    let lines = format!(
                        "{}{} {}",
                        if columns { "Column" } else { "Row" },
                        if count == 1 { "" } else { "s" },
                        if count == 1 {
                            line_label(start)
                        } else {
                            format!("{}–{}", line_label(start), line_label(start + count - 1))
                        }
                    );
                    let region_names = format!(
                        "Region{} {}",
                        if count == 1 { "" } else { "s" },
                        selected
                            .iter()
                            .map(|r| (r + 1).to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    let total = count * board.required();
                    let stars = if total == 1 { "star" } else { "stars" };
                    let needs = if count == 1 { "needs" } else { "need" };
                    let lie = if count == 1 { "lies" } else { "lie" };
                    let region_word = if count == 1 { "region" } else { "regions" };
                    let (clue, explanation) = if regions_inside {
                        (
                            format!(
                                "{region_names} {lie} entirely within {lines}. Both groups need {total} {stars}. Could a star in {lines} lie outside {region_names}?"
                            ),
                            format!(
                                "{region_names} {lie} entirely within {lines} and {needs} {total} {stars} altogether. {lines} also {needs} exactly {total} {stars}, leaving no stars for cells outside {region_names}. {}",
                                action(board, &changes)
                            ),
                        )
                    } else {
                        (
                            format!(
                                "{lines} {needs} {total} {stars}. The {count} {region_word} touching this area also {needs} {total} {stars} altogether. Could any of those stars lie outside {lines}?"
                            ),
                            format!(
                                "{lines} {needs} {total} {stars}. The {count} {region_word} touching this area also {needs} {total} {stars} altogether. All their stars must be inside {lines}, leaving none for their cells outside. {}",
                                action(board, &changes)
                            ),
                        )
                    };
                    best = Some(Step {
                        clue,
                        explanation,
                        focus: (0..n * n).filter(|&i| in_band[i]).collect(),
                        changes,
                        premises: vec![], // The count follows from the layout alone.
                        frames: vec![],
                    });
                }
            }
        }
        if let Some(step) = best {
            return Some((count, step));
        }
    }
    None
}

fn local_step(board: &Board, deadline: Instant) -> Option<Step> {
    if let Some(step) = simple_step(board) {
        return Some(step);
    }
    let mut units = board.units();
    // Check small units first, then compare the complexity of every deduction.
    units.sort_by_key(|(_, cells)| {
        cells
            .iter()
            .filter(|&&i| board.marks[i] == Mark::Empty)
            .count()
    });
    let mut candidates = vec![];
    for (name, cells) in &units {
        if Instant::now() >= deadline {
            break;
        }
        let Some(choices) = placements(board, cells, deadline) else {
            continue;
        };
        // A short, explicit list of cases is a usable explanation. Larger lists
        // are left for the advanced reasoning stage.
        if choices.is_empty() || choices.len() > 6 || choices[0].is_empty() {
            continue;
        }
        let remaining = choices[0].len();
        let star_word = if remaining == 1 { "star" } else { "stars" };
        let options = choices
            .iter()
            .map(|p| format!("• {}", addresses(board, p)))
            .collect::<Vec<_>>()
            .join("\n");
        let premise = format!(
            "{name} needs {remaining} more {}.\n\nPossible placements with no touching stars:\n{options}",
            if remaining == 1 { "star" } else { "stars" }
        );
        let clue = format!(
            "{name} needs {remaining} more {}.\n\nPossible placements:\n{options}\n\nWhich cells are ruled out in every case?",
            if remaining == 1 { "star" } else { "stars" }
        );
        let forced: Vec<_> = choices[0]
            .iter()
            .copied()
            .filter(|i| choices.iter().all(|p| p.contains(i)))
            .map(|i| (i, Mark::Star))
            .collect();
        if !forced.is_empty() {
            candidates.push(((choices.len(), 0, Reverse(forced.len()), cells.len()), Step {
                clue: format!(
                    "{name} needs {remaining} more {star_word}.\n\nPossible placements:\n{options}\n\nWhich cells occur in every placement?"
                ),
                explanation: format!(
                    "{premise}\n\nA star is required wherever all the placements overlap. {}",
                    action(board, &forced)
                ),
                focus: cells.clone(),
                changes: forced,
                premises: premises(board, cells),
                frames: vec![],
            }));
        }
        let excluded: Vec<_> = (0..board.marks.len())
            .filter(|&i| {
                board.marks[i] == Mark::Empty
                    && choices
                        .iter()
                        .all(|p| !p.contains(&i) && p.iter().any(|&j| touching(board, i, j)))
            })
            .map(|i| (i, Mark::Cross))
            .collect();
        if !excluded.is_empty() {
            candidates.push(((choices.len(), 1, Reverse(excluded.len()), cells.len()), Step {
                clue,
                explanation: format!(
                    "{premise}\n\nAny cell next to a star in every placement must be marked with an X. {}",
                    action(board, &excluded)
                ),
                focus: cells.clone(),
                changes: excluded,
                premises: premises(board, cells),
                frames: vec![],
            }));
        }
        // A region's alternatives can reserve the last star(s) in a line, or
        // a line's alternatives can reserve them in a region.
        for (other_name, other) in &units {
            if cells == other {
                continue;
            }
            let minimum = choices
                .iter()
                .map(|p| p.iter().filter(|i| other.contains(i)).count())
                .min()
                .unwrap();
            let fixed = other
                .iter()
                .filter(|&&i| board.marks[i] == Mark::Star)
                .count();
            if minimum == 0 || minimum + fixed != board.required() {
                continue;
            }
            let excluded: Vec<_> = other
                .iter()
                .copied()
                .filter(|&i| {
                    board.marks[i] == Mark::Empty && !choices.iter().any(|p| p.contains(&i))
                })
                .map(|i| (i, Mark::Cross))
                .collect();
            if excluded.is_empty() {
                continue;
            }
            let reserved: Vec<_> = other
                .iter()
                .copied()
                .filter(|i| choices.iter().any(|p| p.contains(i)))
                .collect();
            candidates.push(((choices.len(), 2, Reverse(excluded.len()), cells.len() + other.len()), Step {
                clue: format!(
                    "{name} needs {remaining} more {star_word}.\n\nPossible placements:\n{options}\n\nHow many of those stars must lie in {}?",
                    other_name
                ),
                explanation: format!(
                    "{premise}\n\nEvery placement puts {} in {} among these cells: {}. {} No other cells there can contain stars. {}",
                    counted(minimum, "star"),
                    other_name,
                    addresses(board, &reserved),
                    if fixed == 0 {
                        format!("That accounts for the {} it needs.", counted(board.required(), "star"))
                    } else {
                        format!("Together with the {} already there, that accounts for the {} it needs.", counted(fixed, "star"), counted(board.required(), "star"))
                    },
                    action(board, &excluded)
                ),
                focus: cells.iter().chain(other).copied().collect(),
                changes: excluded,
                premises: premises(
                    board,
                    &cells.iter().chain(other).copied().collect::<Vec<_>>(),
                ),
                frames: vec![],
            }));
        }
    }
    if let Some((groups, step)) = group_count_step(board, deadline) {
        candidates.push((
            (groups, 0, Reverse(step.changes.len()), step.focus.len()),
            step,
        ));
    }
    // Compare every unit, not just the first small one. Fewer cases/groups are easier
    // to follow; for equal case counts prefer direct placements, then neighbors,
    // then quota transfers. Equally simple hints favor more immediate progress.
    candidates
        .into_iter()
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, step)| step)
}

fn trial_step(
    board: &Board,
    cell: usize,
    mark: Mark,
    max_deductions: usize,
    deadline: Instant,
) -> Option<Step> {
    let opposite = if mark == Mark::Star {
        Mark::Cross
    } else {
        Mark::Star
    };
    let assumption = format!(
        "Suppose {} contained {}.",
        board.address(cell),
        if opposite == Mark::Star {
            "a star"
        } else {
            "an X"
        }
    );
    let mut trial = board.clone();
    trial.marks[cell] = opposite;
    let mut frames = vec![ReasoningFrame {
        text: assumption.clone(),
        marks: trial.marks.clone(),
        focus: vec![cell],
        conflicts: vec![],
    }];
    let mut focus = vec![cell];
    let mut first_clue = None;
    for depth in 0..=max_deductions {
        if Instant::now() >= deadline {
            return None;
        }
        let conflict = trial
            .violations()
            .first()
            .map(|v| (v.message.clone(), v.cells.clone()))
            .or_else(|| {
                trial.units().into_iter().find_map(|(name, cells)| {
                    let choices = placements(&trial, &cells, deadline)?;
                    choices.is_empty().then(|| {
                        (
                            if trial.required() == 1 {
                                format!("{name} has no available cell for its star.")
                            } else {
                                format!(
                                    "{name} has no room for {} stars that do not touch.",
                                    trial.required()
                                )
                            },
                            cells,
                        )
                    })
                })
            });
        if let Some((message, cells)) = conflict {
            frames.push(ReasoningFrame {
                text: format!("{message} So our starting assumption cannot be correct."),
                marks: trial.marks.clone(),
                focus: cells.clone(),
                conflicts: cells.clone(),
            });
            focus.extend(cells);
            focus.sort_unstable();
            focus.dedup();
            let changes = vec![(cell, mark)];
            return Some(Step {
                clue: format!(
                    "{assumption} {} Choose Explain Hint to see where this leads.",
                    first_clue.unwrap_or_else(|| message.clone())
                ),
                explanation: format!(
                    "{assumption} {message}\n\nThat rules out {} at {}. {}",
                    if opposite == Mark::Star {
                        "a star"
                    } else {
                        "an X"
                    },
                    board.address(cell),
                    action(board, &changes)
                ),
                focus,
                changes,
                premises: premises(board, &(0..board.marks.len()).collect::<Vec<_>>()),
                frames,
            });
        }
        if depth == max_deductions {
            break;
        }
        let step = local_step(&trial, deadline)?;
        if first_clue.is_none() {
            first_clue = Some(step.clue.clone());
            focus.extend(&step.focus);
        }
        for &(i, mark) in &step.changes {
            trial.marks[i] = mark;
        }
        frames.push(ReasoningFrame {
            text: step.explanation,
            marks: trial.marks.clone(),
            focus: step.focus,
            conflicts: vec![],
        });
    }
    None
}

pub fn advise(board: &Board, budget: Duration) -> Advice {
    let deadline = Instant::now() + budget;
    if !board.violations().is_empty() {
        return diagnose(board, deadline);
    }
    if board.solved() {
        return Advice::Solved;
    }
    let mut search = Search::new(board, deadline);
    let domains = search.domains(None);
    let solution = match search.run(domains) {
        SearchResult::Solution(s) => s,
        SearchResult::Impossible => return diagnose(board, deadline),
        SearchResult::Limit => return Advice::Limit,
    };
    if let Some(step) = local_step(board, deadline) {
        return Advice::Step(step);
    }
    let mut domains = search.domains(None);
    if search.propagate(&mut domains) != Ok(true) {
        return Advice::Limit;
    }
    // Search may suggest which assumptions to examine, but a teaching step is
    // reconstructed from the visible board using only the local rules above.
    // Prefer a short, replayable chain before falling back to exhaustive search.
    let trial_deadline = deadline.min(Instant::now() + budget / 2);
    let mut candidates: Vec<_> = (0..board.marks.len())
        .filter(|&i| board.marks[i] == Mark::Empty)
        .collect();
    candidates.sort_by_key(|&i| {
        (
            domains[i / board.size].len(),
            solution[i / board.size] & (1 << (i % board.size)) == 0,
        )
    });
    // Check every cell for an immediate contradiction before trying any chain,
    // then check one deduction, two, etc. Board order cannot hide a shorter proof
    // behind the first successful long trial.
    'trials: for max_deductions in 0..16 {
        for &i in &candidates {
            if Instant::now() >= trial_deadline {
                break 'trials;
            }
            let mark = if solution[i / board.size] & (1 << (i % board.size)) != 0 {
                Mark::Star
            } else {
                Mark::Cross
            };
            if let Some(step) = trial_step(board, i, mark, max_deductions, trial_deadline) {
                return Advice::Step(step);
            }
        }
    }
    // Prove a forced move by ruling out its opposite. Never expose an arbitrary solution as a hint.
    let mut cells: Vec<_> = (0..board.marks.len()).collect();
    cells.sort_by_key(|&i| {
        (
            domains[i / board.size].len(),
            solution[i / board.size] & (1 << (i % board.size)) == 0,
        )
    });
    for i in cells {
        if board.marks[i] != Mark::Empty {
            continue;
        }
        let mark = if solution[i / board.size] & (1 << (i % board.size)) != 0 {
            Mark::Star
        } else {
            Mark::Cross
        };
        let opposite = if mark == Mark::Star {
            Mark::Cross
        } else {
            Mark::Star
        };
        let mut branch = domains.clone();
        branch[i / board.size]
            .retain(|&mask| (mask & (1 << (i % board.size)) != 0) == (opposite == Mark::Star));
        match search.run(branch) {
            SearchResult::Impossible => {
                let mut assumed = board.clone();
                assumed.marks[i] = opposite;
                let mut proof_search = Search::new(&assumed, deadline);
                proof_search.trace = Some(RefCell::new(vec![ReasoningFrame {
                    text: format!(
                        "Suppose {} contained {}. Each row still needs exactly {}, with no touching stars and all your other marks kept.",
                        board.address(i),
                        if opposite == Mark::Star {
                            "a star"
                        } else {
                            "an X"
                        },
                        counted(board.required(), "star")
                    ),
                    marks: assumed.marks.clone(),
                    focus: vec![i],
                    conflicts: vec![],
                }]));
                let initial = proof_search.domains(None);
                for (r, patterns) in initial.iter().enumerate() {
                    proof_search.record(&initial, proof_search.row_cells(r), false, || {
                        format!(
                            "Possible placements in row {}: {}.",
                            Board::row_label(r),
                            proof_search.describe(r, patterns)
                        )
                    });
                }
                if !matches!(proof_search.run(initial), SearchResult::Impossible)
                    || proof_search.expired()
                {
                    return Advice::Limit;
                }
                let frames = proof_search.trace.take().unwrap().into_inner();
                let changes = vec![(i, mark)];
                return Advice::Step(Step {
                    clue: format!(
                        "This hint takes a few more steps. What would happen if you placed {} at {}? Choose Explain Hint to explore the possibilities.",
                        if opposite == Mark::Star {
                            "a star"
                        } else {
                            "an X"
                        },
                        board.address(i)
                    ),
                    explanation: format!(
                        "Every arrangement with {} at {} breaks a rule. {}",
                        if opposite == Mark::Star {
                            "a star"
                        } else {
                            "an X"
                        },
                        board.address(i),
                        action(board, &changes)
                    ),
                    focus: vec![i],
                    changes,
                    premises: premises(board, &(0..board.marks.len()).collect::<Vec<_>>()),
                    frames,
                });
            }
            SearchResult::Limit => return Advice::Limit,
            SearchResult::Solution(_) => {}
        }
    }
    Advice::NoForcedMove
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_counts_exclude_cells_in_both_directions_and_axes() {
        let original: Board = serde_json::from_str(include_str!(
            "../tests/fixtures/10x10-counting-position.json"
        ))
        .unwrap();
        for regions_inside in [false, true] {
            for transpose in [false, true] {
                let mut board = original.clone();
                if regions_inside {
                    for i in [60, 62, 63] {
                        board.marks[i] = Mark::Cross;
                    }
                }
                let transform = |i| if transpose { (i % 10) * 10 + i / 10 } else { i };
                if transpose {
                    let before = board.clone();
                    for i in 0..100 {
                        board.regions[transform(i)] = before.regions[i];
                        board.marks[transform(i)] = before.marks[i];
                    }
                }
                board.validate().unwrap();
                let (count, step) =
                    group_count_step(&board, Instant::now() + Duration::from_secs(2)).unwrap();
                let mut expected: Vec<_> = if regions_inside {
                    vec![22, 32, 34, 36]
                } else {
                    vec![60, 62, 63]
                }
                .into_iter()
                .map(|i| (transform(i), Mark::Cross))
                .collect();
                expected.sort_by_key(|&(i, _)| i);
                assert_eq!(count, if regions_inside { 4 } else { 3 });
                assert_eq!(step.changes, expected);
                assert!(step.frames.is_empty());
                assert!(step.premises.is_empty()); // It remains true when other marks change.
                assert!(
                    step.explanation
                        .contains(if transpose { "Columns" } else { "Rows" })
                );

                // Verify the exclusions without relying on any player marks.
                board.marks.fill(Mark::Empty);
                for (i, _) in step.changes {
                    board.marks[i] = Mark::Star;
                    assert!(matches!(
                        solve(&board, Duration::from_secs(2)),
                        SearchResult::Impossible
                    ));
                    board.marks[i] = Mark::Empty;
                }
            }
        }
    }

    #[test]
    fn unequal_group_counts_do_not_create_exclusions() {
        // Every row intersects all five vertical strip regions.
        // Matching a region to its identical column also has no outside cells.
        let board = Board::new(5, (0..25).map(|i| i % 5).collect(), vec![Mark::Empty; 25]).unwrap();
        assert!(group_count_step(&board, Instant::now() + Duration::from_secs(2)).is_none());
    }

    #[test]
    fn advanced_proof_names_the_count_that_fails() {
        let mut board =
            Board::new(5, (0..25).map(|i| i / 5).collect(), vec![Mark::Empty; 25]).unwrap();
        board.marks[0] = Mark::Star;
        board.marks[10] = Mark::Star;
        let mut search = Search::new(&board, Instant::now() + Duration::from_secs(2));
        search.trace = Some(RefCell::new(vec![]));
        let domains = search.domains(None);
        assert!(matches!(search.run(domains), SearchResult::Impossible));
        let proof = search.trace.unwrap().into_inner();
        assert!(
            proof
                .iter()
                .any(|s| s.text.starts_with("Column 1 needs 1 star,")
                    && s.conflicts == vec![0, 5, 10, 15, 20]
                    && s.marks[0] == Mark::Star
                    && s.marks[10] == Mark::Star)
        );
    }
}
