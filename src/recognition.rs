//! Pixel-only recognition for the colored boards at starbattlepuzzles.com.
//! No page access, network service, OCR, or hidden solution data is used.
use crate::model::{Board, Mark};
use image::RgbImage;
use std::collections::VecDeque;

// The site's light and dark region palettes, observed from rendered cells.
const PALETTE: [[u8; 3]; 28] = [
    [195, 222, 248],
    [251, 217, 182],
    [192, 237, 207],
    [227, 207, 247],
    [247, 201, 201],
    [250, 235, 158],
    [188, 235, 235],
    [245, 209, 233],
    [212, 233, 190],
    [214, 214, 245],
    [245, 202, 188],
    [198, 236, 223],
    [238, 211, 238],
    [239, 239, 190],
    [36, 66, 96],
    [96, 66, 36],
    [40, 83, 54],
    [71, 46, 96],
    [96, 41, 41],
    [89, 80, 34],
    [35, 82, 82],
    [93, 45, 77],
    [61, 83, 40],
    [48, 48, 100],
    [93, 53, 40],
    [40, 83, 68],
    [90, 48, 90],
    [82, 82, 35],
];

pub struct Recognized {
    pub board: Board,
    pub crop: [u32; 4],
    pub warnings: Vec<String>,
}

fn distance(a: [u8; 3], b: [u8; 3]) -> u32 {
    (0..3)
        .map(|i| {
            let d = a[i] as i32 - b[i] as i32;
            (d * d) as u32
        })
        .sum()
}
fn colorful(p: [u8; 3]) -> bool {
    let max = *p.iter().max().unwrap();
    let min = *p.iter().min().unwrap();
    max - min >= 24 && max > 55
}

fn board_bounds(image: &RgbImage) -> Result<[u32; 4], String> {
    let (width, height) = image.dimensions();
    if width < 100 || height < 100 || width as u64 * height as u64 > 40_000_000 {
        return Err(
            "Use a screenshot between 100 pixels and 40 megapixels, with the whole board visible."
                .into(),
        );
    }
    let stride = 4usize;
    let w = width as usize / stride;
    let h = height as usize / stride;
    let mut mask = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            mask[y * w + x] = colorful(image.get_pixel((x * stride) as u32, (y * stride) as u32).0);
        }
    }
    let mut best = None;
    let mut best_count = 0;
    for start in 0..mask.len() {
        if !mask[start] {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        mask[start] = false;
        let (mut left, mut top, mut right, mut bottom) = (w, h, 0, 0);
        let mut count = 0;
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % w, i / w);
            count += 1;
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
            // Bridge thin neutral grid lines, but not the gap to surrounding controls.
            for (dx, dy) in [
                (-2, 0),
                (-1, 0),
                (1, 0),
                (2, 0),
                (0, -2),
                (0, -1),
                (0, 1),
                (0, 2),
            ] {
                let xx = x as isize + dx;
                let yy = y as isize + dy;
                if xx >= 0 && yy >= 0 && xx < w as isize && yy < h as isize {
                    let j = yy as usize * w + xx as usize;
                    if mask[j] {
                        mask[j] = false;
                        queue.push_back(j);
                    }
                }
            }
        }
        let bw = right - left + 1;
        let bh = bottom - top + 1;
        if count > best_count
            && bw > 20
            && bh > 20
            && (bw as f32 / bh as f32 - 1.).abs() < 0.08
            && count * 2 > bw * bh
        {
            best_count = count;
            best = Some([left, top, right, bottom]);
        }
    }
    let Some([l, t, r, b]) = best else {
        return Err("No complete colored board found. Dismiss the site's Play overlay and copy the whole board, including its outer edges.".into());
    };
    let (l, t, r, b) = (
        (l * stride).saturating_sub(stride) as u32,
        (t * stride).saturating_sub(stride) as u32,
        ((r + 2) * stride).min(width as usize) as u32,
        ((b + 2) * stride).min(height as usize) as u32,
    );
    let mut xs = Vec::new();
    let mut xe = Vec::new();
    let mut ys = Vec::new();
    let mut ye = Vec::new();
    // Crop to cell backgrounds rather than the site's colored outer frame or
    // shadow. This also keeps the source preview aligned with generated cells.
    let is_region_color = |x, y| {
        let pixel = image.get_pixel(x, y).0;
        colorful(pixel) && PALETTE.iter().any(|&rgb| distance(pixel, rgb) <= 900)
    };
    for y in t..b {
        if let Some(x) = (l..r).find(|&x| is_region_color(x, y)) {
            xs.push(x);
        }
        if let Some(x) = (l..r).rev().find(|&x| is_region_color(x, y)) {
            xe.push(x + 1);
        }
    }
    for x in l..r {
        if let Some(y) = (t..b).find(|&y| is_region_color(x, y)) {
            ys.push(y);
        }
        if let Some(y) = (t..b).rev().find(|&y| is_region_color(x, y)) {
            ye.push(y + 1);
        }
    }
    if xs.is_empty() || xe.is_empty() || ys.is_empty() || ye.is_empty() {
        return Err("No board with recognizable region colors found.".into());
    }
    for values in [&mut xs, &mut xe, &mut ys, &mut ye] {
        values.sort_unstable();
    }
    let left = xs[xs.len() / 10];
    let top = ys[ys.len() / 10];
    let right = xe[xe.len() * 9 / 10];
    let bottom = ye[ye.len() * 9 / 10];
    Ok([left, top, right - left, bottom - top])
}

fn cell_color(image: &RgbImage, x: f32, y: f32, cw: f32, ch: f32) -> ([u8; 3], usize) {
    let mut samples = Vec::new();
    for fy in [0.14, 0.24, 0.76, 0.86] {
        for fx in [0.14, 0.24, 0.76, 0.86] {
            samples.push(
                image
                    .get_pixel((x + fx * cw) as u32, (y + fy * ch) as u32)
                    .0,
            );
        }
    }
    let color = *samples
        .iter()
        .max_by_key(|&&a| samples.iter().filter(|&&b| distance(a, b) < 225).count())
        .unwrap();
    let matches: Vec<_> = samples
        .iter()
        .copied()
        .filter(|&b| distance(color, b) < 225)
        .collect();
    let mut median = [0; 3];
    for channel in 0..3 {
        let mut values: Vec<_> = matches.iter().map(|p| p[channel]).collect();
        values.sort_unstable();
        median[channel] = values[values.len() / 2];
    }
    (median, matches.len())
}

fn read_mark(
    image: &RgbImage,
    x: f32,
    y: f32,
    cw: f32,
    ch: f32,
    bg: [u8; 3],
) -> Result<(Mark, bool), String> {
    let mut ink = 0usize;
    let mut ink_points = Vec::new();
    let mut total = 0usize;
    let (mut minx, mut maxx, mut miny, mut maxy) = (u32::MAX, 0, u32::MAX, 0);
    // Restrict to the central area, excluding grid lines and focus outlines.
    for yy in (y + ch * 0.30) as u32..(y + ch * 0.70) as u32 {
        for xx in (x + cw * 0.30) as u32..(x + cw * 0.70) as u32 {
            total += 1;
            if distance(image.get_pixel(xx, yy).0, bg) > 900 {
                ink += 1;
                ink_points.push((xx, yy));
                minx = minx.min(xx);
                maxx = maxx.max(xx);
                miny = miny.min(yy);
                maxy = maxy.max(yy);
            }
        }
    }
    let ratio = ink as f32 / total.max(1) as f32;
    if ratio > 0.47 {
        return Ok((Mark::Star, false));
    }
    if ratio < 0.015 {
        return Ok((Mark::Empty, false));
    }
    let extent = ((maxx - minx + 1) as f32 / cw).max((maxy - miny + 1) as f32 / ch);
    if extent < 0.17 {
        return Ok((Mark::Empty, true));
    }
    let center_x = (minx + maxx + 1) as f32 / 2.;
    let center_y = (miny + maxy + 1) as f32 / 2.;
    let diagonal_ink = ink_points
        .iter()
        .filter(|&&(xx, yy)| {
            let dx = (xx as f32 + 0.5 - center_x) / cw;
            let dy = (yy as f32 + 0.5 - center_y) / ch;
            (dx.abs() - dy.abs()).abs() < 0.065
        })
        .count();
    if ratio < 0.42 && (0.18..0.37).contains(&extent) && diagonal_ink * 100 >= ink * 80 {
        return Ok((Mark::Cross, false));
    }
    Err("A cell's mark is unclear. Use a sharper screenshot without a selection or cursor covering the board.".into())
}

pub fn recognize(image: &RgbImage) -> Result<Recognized, String> {
    let crop = board_bounds(image)?;
    let [left, top, width, height] = crop;
    let mut candidates = Vec::new();
    for n in [5, 8, 10, 14] {
        let cw = width as f32 / n as f32;
        let ch = height as f32 / n as f32;
        if cw < 15. || ch < 15. {
            continue;
        }
        let mut palette: Vec<usize> = Vec::new();
        let mut regions = Vec::new();
        let mut colors = Vec::new();
        let mut bad = 0;
        for r in 0..n {
            for c in 0..n {
                let (color, matches) = cell_color(
                    image,
                    left as f32 + c as f32 * cw,
                    top as f32 + r as f32 * ch,
                    cw,
                    ch,
                );
                bad += 16 - matches;
                if !colorful(color) {
                    bad += 16;
                }
                let nearest = (0..PALETTE.len())
                    .min_by_key(|&i| distance(PALETTE[i], color))
                    .unwrap();
                if distance(PALETTE[nearest], color) > 300 {
                    bad += 16;
                }
                let region = palette
                    .iter()
                    .position(|&p| p == nearest)
                    .unwrap_or_else(|| {
                        palette.push(nearest);
                        palette.len() - 1
                    });
                regions.push(region);
                colors.push(color);
            }
        }
        if palette.len() != n || bad > n * n {
            continue;
        }
        let mut marks = Vec::new();
        let mut ignored = 0;
        let mut error = false;
        for (i, &color) in colors.iter().enumerate() {
            match read_mark(
                image,
                left as f32 + (i % n) as f32 * cw,
                top as f32 + (i / n) as f32 * ch,
                cw,
                ch,
                color,
            ) {
                Ok((mark, dot)) => {
                    marks.push(mark);
                    ignored += usize::from(dot);
                }
                Err(_) => {
                    error = true;
                    break;
                }
            }
        }
        if error {
            continue;
        }
        if let Ok(mut board) = Board::new(n, regions, marks) {
            board.region_colors = palette.iter().map(|&i| PALETTE[i]).collect();
            let warnings = if ignored > 0 {
                vec![format!(
                    "Ignored {ignored} small dot notes. Please review the imported marks."
                )]
            } else {
                Vec::new()
            };
            candidates.push((
                bad,
                Recognized {
                    board,
                    crop,
                    warnings,
                },
            ));
        }
    }
    candidates.sort_by_key(|(bad, _)| *bad);
    if candidates.len() > 1 && candidates[0].0 == candidates[1].0 {
        return Err(
            "The board size is ambiguous. Paste a sharper screenshot of the full board.".into(),
        );
    }
    candidates.into_iter().next().map(|(_,r)|r).ok_or_else(||"The board could not be read confidently. Use an unobstructed starbattlepuzzles.com board in its normal light or dark theme (5×5, 8×8, 10×10, or 14×14).".into())
}
