use star_battle_solver::model::{Board, Mark};
use std::collections::HashMap;

pub fn expected(name: &str) -> Board {
    let text = std::fs::read_to_string(format!("tests/fixtures/{name}.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let cells = json["cells"].as_array().unwrap();
    let n = (cells.len() as f32).sqrt() as usize;
    let mut ids = HashMap::new();
    let mut regions = Vec::new();
    let mut colors = Vec::new();
    let mut marks = Vec::new();
    for cell in cells {
        let label = cell["label"].as_str().unwrap();
        let original: usize = label
            .split("region ")
            .nth(1)
            .unwrap()
            .split(':')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let next = ids.len();
        regions.push(*ids.entry(original).or_insert_with(|| {
            let color = cell["color"].as_str().unwrap();
            let channels: Vec<u8> = color
                .trim_start_matches("rgb(")
                .trim_end_matches(')')
                .split(',')
                .map(|channel| channel.trim().parse().unwrap())
                .collect();
            colors.push(channels.try_into().unwrap());
            next
        }));
        marks.push(if label.ends_with(": star") {
            Mark::Star
        } else if label.ends_with(": marked empty") {
            Mark::Cross
        } else {
            Mark::Empty
        });
    }
    let mut board = Board::new(n, regions, marks).unwrap();
    board.region_colors = colors;
    board
}
