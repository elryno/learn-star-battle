use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mark {
    #[default]
    Empty,
    Cross,
    Star,
}

impl Mark {
    pub fn next(self) -> Self {
        match self {
            Self::Empty => Self::Cross,
            Self::Cross => Self::Star,
            Self::Star => Self::Empty,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub size: usize,
    pub regions: Vec<usize>,
    /// Screenshot colors indexed by region. Empty for older saved boards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub region_colors: Vec<[u8; 3]>,
    pub marks: Vec<Mark>,
    /// Only these X's are derived from stars. Other X's belong to the player.
    #[serde(default)]
    pub automatic_crosses: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Violation {
    pub cells: Vec<usize>,
    pub message: String,
}

pub fn star_count(size: usize) -> Option<usize> {
    match size {
        5 | 8 => Some(1),
        10 => Some(2),
        14 => Some(3),
        _ => None,
    }
}

impl Board {
    pub fn new(size: usize, regions: Vec<usize>, marks: Vec<Mark>) -> Result<Self, String> {
        let board = Self {
            size,
            regions,
            region_colors: vec![],
            marks,
            automatic_crosses: vec![],
        };
        board.validate()?;
        Ok(board)
    }

    pub fn validate(&self) -> Result<(), String> {
        let n = self.size;
        if star_count(n).is_none() {
            return Err("Supported sizes are 5×5, 8×8, 10×10, and 14×14.".into());
        }
        if self.regions.len() != n * n
            || self.marks.len() != n * n
            || self.regions.iter().any(|&r| r >= n)
        {
            return Err("The board needs one region and one mark per cell.".into());
        }
        if !self.region_colors.is_empty() && self.region_colors.len() != n {
            return Err("The board needs one color per region.".into());
        }
        if self
            .automatic_crosses
            .iter()
            .any(|&i| i >= n * n || self.marks[i] != Mark::Cross)
            || self
                .automatic_crosses
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            return Err("Automatic X markers are invalid.".into());
        }
        for region in 0..n {
            let cells: Vec<_> = (0..n * n).filter(|&i| self.regions[i] == region).collect();
            let Some(&start) = cells.first() else {
                return Err(format!("Region {} is missing.", region + 1));
            };
            let mut seen = vec![false; n * n];
            let mut queue = VecDeque::from([start]);
            seen[start] = true;
            while let Some(i) = queue.pop_front() {
                for j in self
                    .neighbors(i)
                    .into_iter()
                    .filter(|&j| i / n == j / n || i % n == j % n)
                {
                    if !seen[j] && self.regions[j] == region {
                        seen[j] = true;
                        queue.push_back(j);
                    }
                }
            }
            if cells.iter().any(|&i| !seen[i]) {
                return Err(format!(
                    "Region {} must be connected along cell edges.",
                    region + 1
                ));
            }
        }
        Ok(())
    }

    pub fn required(&self) -> usize {
        star_count(self.size).expect("validated board size")
    }

    pub fn play_marks(&mut self, changes: &[(usize, Mark)], auto_crosses: bool) {
        self.set_marks(changes);
        self.refresh_automatic_crosses(auto_crosses);
    }
    fn set_marks(&mut self, changes: &[(usize, Mark)]) {
        // Explicitly marking an automatic X with X promotes it to a manual note.
        self.automatic_crosses
            .retain(|i| !changes.iter().any(|(j, _)| i == j));
        for &(i, mark) in changes {
            self.marks[i] = mark;
        }
    }

    pub fn refresh_automatic_crosses(&mut self, enabled: bool) {
        for i in self.automatic_crosses.drain(..) {
            if self.marks[i] == Mark::Cross {
                self.marks[i] = Mark::Empty;
            }
        }
        if !enabled {
            return;
        }
        let mut blocked = vec![false; self.marks.len()];
        for (i, mark) in self.marks.iter().enumerate() {
            if *mark == Mark::Star {
                for j in self.neighbors(i) {
                    blocked[j] = true;
                }
            }
        }
        // Fill completed rows, columns, and regions using the puzzle's quota.
        for (_, cells) in self.units() {
            if cells
                .iter()
                .filter(|&&i| self.marks[i] == Mark::Star)
                .count()
                == self.required()
            {
                for i in cells {
                    blocked[i] = true;
                }
            }
        }
        for (i, blocked) in blocked.into_iter().enumerate() {
            if blocked && self.marks[i] == Mark::Empty {
                self.marks[i] = Mark::Cross;
                self.automatic_crosses.push(i);
            }
        }
    }
    pub fn address(&self, cell: usize) -> String {
        format!(
            "{}{}",
            Self::row_label(cell / self.size),
            cell % self.size + 1
        )
    }

    pub fn row_label(row: usize) -> char {
        (b'A' + row as u8) as char
    }

    pub fn neighbors(&self, cell: usize) -> Vec<usize> {
        let (r, c) = (cell / self.size, cell % self.size);
        let mut out = Vec::with_capacity(8);
        for rr in r.saturating_sub(1)..=(r + 1).min(self.size - 1) {
            for cc in c.saturating_sub(1)..=(c + 1).min(self.size - 1) {
                if rr != r || cc != c {
                    out.push(rr * self.size + cc);
                }
            }
        }
        out
    }

    pub fn units(&self) -> Vec<(String, Vec<usize>)> {
        let n = self.size;
        let mut units = Vec::with_capacity(n * 3);
        for r in 0..n {
            units.push((
                format!("Row {}", Self::row_label(r)),
                (r * n..r * n + n).collect(),
            ));
        }
        for c in 0..n {
            units.push((
                format!("Column {}", c + 1),
                (0..n).map(|r| r * n + c).collect(),
            ));
        }
        for region in 0..n {
            units.push((
                format!("Region {}", region + 1),
                (0..n * n).filter(|&i| self.regions[i] == region).collect(),
            ));
        }
        units
    }

    pub fn violations(&self) -> Vec<Violation> {
        let mut violations = Vec::new();
        for i in 0..self.marks.len() {
            if self.marks[i] != Mark::Star {
                continue;
            }
            for j in self.neighbors(i) {
                if j > i && self.marks[j] == Mark::Star {
                    violations.push(Violation {
                        cells: vec![i, j],
                        message: format!(
                            "Stars at {} and {} touch.",
                            self.address(i),
                            self.address(j)
                        ),
                    });
                }
            }
        }
        for (name, cells) in self.units() {
            let stars: Vec<_> = cells
                .iter()
                .copied()
                .filter(|&i| self.marks[i] == Mark::Star)
                .collect();
            let available = cells
                .iter()
                .filter(|&&i| {
                    self.marks[i] == Mark::Empty
                        && self
                            .neighbors(i)
                            .iter()
                            .all(|&j| self.marks[j] != Mark::Star)
                })
                .count();
            if stars.len() > self.required() {
                violations.push(Violation {
                    cells,
                    message: format!(
                        "{name} has {} stars; it needs exactly {}. Remove {} {}.",
                        stars.len(),
                        self.required(),
                        stars.len() - self.required(),
                        if stars.len() - self.required() == 1 {
                            "star"
                        } else {
                            "stars"
                        },
                    ),
                });
            } else if stars.len() + available < self.required() {
                violations.push(Violation {
                    cells,
                    message: format!(
                        "{name} does not have enough space for its {} {}.",
                        self.required(),
                        if self.required() == 1 {
                            "star"
                        } else {
                            "stars"
                        }
                    ),
                });
            }
        }
        violations
    }

    pub fn solved(&self) -> bool {
        self.marks.iter().filter(|&&m| m == Mark::Star).count() == self.size * self.required()
            && self.violations().is_empty()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Game {
    pub board: Option<Board>,
    undo: Vec<Board>,
    redo: Vec<Board>,
    #[serde(default = "default_enabled")]
    pub auto_crosses: bool,
    #[serde(default = "default_enabled")]
    pub show_errors: bool,
}

fn default_enabled() -> bool {
    true
}

impl Default for Game {
    fn default() -> Self {
        Self {
            board: None,
            undo: vec![],
            redo: vec![],
            auto_crosses: true,
            show_errors: true,
        }
    }
}

impl Game {
    pub fn load(&mut self, mut board: Board) {
        board.refresh_automatic_crosses(self.auto_crosses);
        self.board = Some(board);
        self.undo.clear();
        self.redo.clear();
    }
    pub fn change(&mut self, change: impl FnOnce(&mut Board)) {
        if let Some(board) = self.board.as_mut() {
            let before = board.clone();
            change(board);
            board.refresh_automatic_crosses(self.auto_crosses);
            if *board != before {
                self.undo.push(before);
                self.redo.clear();
                if self.undo.len() > 500 {
                    self.undo.remove(0);
                }
            }
        }
    }
    pub fn play(&mut self, changes: &[(usize, Mark)]) {
        self.change(|board| board.set_marks(changes));
    }
    pub fn set_auto_crosses(&mut self, enabled: bool) {
        self.auto_crosses = enabled;
        self.refresh_automatic_crosses();
    }
    pub fn refresh_automatic_crosses(&mut self) {
        if let Some(board) = &mut self.board {
            board.refresh_automatic_crosses(self.auto_crosses);
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self) {
        if let Some(old) = self.undo.pop() {
            if let Some(current) = self.board.replace(old) {
                self.redo.push(current);
            }
        }
        self.refresh_automatic_crosses();
    }
    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            if let Some(current) = self.board.replace(next) {
                self.undo.push(current);
            }
        }
        self.refresh_automatic_crosses();
    }
    pub fn valid(&self) -> bool {
        self.board
            .iter()
            .chain(self.undo.iter())
            .chain(self.redo.iter())
            .all(|b| b.validate().is_ok())
    }
}
