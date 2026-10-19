mod common;
use common::expected;
use star_battle_solver::recognition::recognize;
use std::path::Path;

#[test]
fn site_screenshots_preserve_every_region_color_and_mark() {
    for name in [
        "5x5-empty",
        "5x5-partial",
        "5x5-dark-partial",
        "8x8-empty",
        "10x10-empty",
        "14x14-empty",
        "14x14-partial",
        "14x14-dark-partial",
    ] {
        let path = format!("tests/fixtures/{name}.jpg");
        let image = image::open(Path::new(&path)).unwrap().to_rgb8();
        let detected = recognize(&image).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(detected.board, expected(name), "{name}");
    }
}

#[test]
fn locates_board_inside_full_page() {
    let image = image::open("tests/fixtures/10x10-daily-page.png")
        .unwrap()
        .to_rgb8();
    let detected = recognize(&image).unwrap();
    let expected: star_battle_solver::model::Board =
        serde_json::from_str(include_str!("fixtures/10x10-daily-page.json")).unwrap();
    assert_eq!(detected.board, expected);
    let [x, y, width, height] = detected.crop;
    assert!((340..360).contains(&x));
    assert!((200..220).contains(&y));
    assert!((560..580).contains(&width));
    assert!((560..580).contains(&height));
}

#[test]
fn scaled_screenshots_are_supported() {
    let source = image::open("tests/fixtures/14x14-partial.jpg")
        .unwrap()
        .to_rgb8();
    let [x, y, w, h] = recognize(&source).unwrap().crop;
    let source = image::imageops::crop_imm(&source, x, y, w, h).to_image();
    for side in [420, 800] {
        let resized =
            image::imageops::resize(&source, side, side, image::imageops::FilterType::Triangle);
        assert_eq!(
            recognize(&resized)
                .unwrap_or_else(|e| panic!("size {side}: {e}"))
                .board,
            expected("14x14-partial")
        );
    }
}

#[test]
fn unrelated_and_incomplete_images_are_rejected() {
    let blank = image::RgbImage::from_pixel(600, 600, image::Rgb([255, 255, 255]));
    assert!(recognize(&blank).is_err());
    let source = image::open("tests/fixtures/10x10-empty.jpg")
        .unwrap()
        .to_rgb8();
    let partial =
        image::imageops::crop_imm(&source, 0, 0, source.width() / 2, source.height()).to_image();
    assert!(recognize(&partial).is_err());
}

#[test]
fn resized_boards_with_page_margins_preserve_every_cell() {
    for name in [
        "5x5-partial",
        "5x5-dark-partial",
        "8x8-empty",
        "10x10-empty",
        "14x14-partial",
        "14x14-dark-partial",
    ] {
        let source = image::open(format!("tests/fixtures/{name}.jpg"))
            .unwrap()
            .to_rgb8();
        let [x, y, w, h] = recognize(&source).unwrap().crop;
        let crop = image::imageops::crop_imm(&source, x, y, w, h).to_image();
        let expected = expected(name);
        for cell in [20, 30, 45] {
            let side = expected.size as u32 * cell;
            let resized =
                image::imageops::resize(&crop, side, side, image::imageops::FilterType::Triangle);
            let bg = if name.contains("dark") {
                [24, 24, 24]
            } else {
                [250, 249, 247]
            };
            let mut page = image::RgbImage::from_pixel(side + 173, side + 251, image::Rgb(bg));
            image::imageops::replace(&mut page, &resized, 67, 113);
            let found = recognize(&page).unwrap_or_else(|e| panic!("{name}, cell={cell}: {e}"));
            assert_eq!(found.board, expected, "{name}, cell={cell}");
            assert!(found.crop[0].abs_diff(67) <= 2 && found.crop[1].abs_diff(113) <= 2);
            assert!(found.crop[2].abs_diff(side) <= 2 && found.crop[3].abs_diff(side) <= 2);
        }
    }
}

#[test]
fn tiny_and_palette_colored_non_boards_are_rejected_without_panics() {
    for (w, h) in [(0, 0), (1, 1), (99, 200), (200, 99), (100, 100), (500, 500)] {
        for color in [[0, 0, 0], [195, 222, 248], [245, 209, 233]] {
            let image = image::RgbImage::from_pixel(w, h, image::Rgb(color));
            assert!(recognize(&image).is_err(), "{w}x{h}: {color:?}");
        }
    }
}

#[test]
fn low_resolution_imports_are_correct_or_rejected_instead_of_losing_marks() {
    for name in [
        "5x5-partial",
        "5x5-dark-partial",
        "14x14-partial",
        "14x14-dark-partial",
    ] {
        let source = image::open(format!("tests/fixtures/{name}.jpg"))
            .unwrap()
            .to_rgb8();
        let [x, y, w, h] = recognize(&source).unwrap().crop;
        let crop = image::imageops::crop_imm(&source, x, y, w, h).to_image();
        let expected = expected(name);
        for cell in [15, 16, 18] {
            let side = expected.size as u32 * cell;
            let resized =
                image::imageops::resize(&crop, side, side, image::imageops::FilterType::Triangle);
            if let Ok(found) = recognize(&resized) {
                assert_eq!(
                    found.board, expected,
                    "Accepted incorrect {name} at {cell}px per cell"
                );
            }
        }
    }
}
