//! The tray's icon, drawn in code at whatever size the tray asks: the Mac
//! app's rings, the 5-hour window outside (green, then orange from 70%,
//! red from 90%) and the 7-day inside (light purple, deep purple, magenta),
//! each over a faint track. Without a 5-hour window the 7-day takes the
//! outer ring alone; without any usage both are tracks. Each pixel's
//! coverage is worked out from its distance to the ring and its angle, so
//! the edges are smooth at 16 pixels as at 64. With a number
//! (docs/design/tray-numbers.md), the rings go thin at the edge and the
//! outer ring's percentage sits in the middle, in a pixel font of U4Ai's
//! own drawn on whole pixels. Straight (not premultiplied) RGBA, rows top
//! to bottom.

use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// One ring: how full (0–1) and its color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring {
    pub fill: f64,
    pub color: Rgb,
}

/// What the icon shows: an outer ring and maybe an inner one; none at all
/// is the placeholder (tracks alone). `number` (0–100) is drawn in the
/// middle in the outer ring's color, where there's an outer ring.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rings {
    pub outer: Option<Ring>,
    pub inner: Option<Ring>,
    pub number: Option<u32>,
}

/// The digits, 3 × 5 pixels each (the 1 two wide), a row a string.
pub const DIGITS: [[&str; 5]; 10] = [
    ["###", "#.#", "#.#", "#.#", "###"],
    [".#", "##", ".#", ".#", ".#"],
    ["###", "..#", "###", "#..", "###"],
    ["###", "..#", "###", "..#", "###"],
    ["#.#", "#.#", "###", "..#", "..#"],
    ["###", "#..", "###", "..#", "###"],
    ["###", "#..", "###", "#.#", "###"],
    ["###", "..#", "..#", "..#", "..#"],
    ["###", "#.#", "###", "#.#", "###"],
    ["###", "#.#", "###", "..#", "###"],
];

/// The font's height in pixels, and the space between digits.
const DIGIT_HEIGHT: usize = 5;
const DIGIT_GAP: usize = 1;

/// How wide `text`'s digits are at scale 1, gaps between them.
fn text_width(text: &str) -> usize {
    let widths: Vec<usize> = text.bytes().map(|b| DIGITS[usize::from(b - b'0')][0].len()).collect();
    widths.iter().sum::<usize>() + DIGIT_GAP * widths.len().saturating_sub(1)
}

/// Where the rings go at size `s`: their width, the outer's and inner's
/// middle lines, and the radius of the hole they leave.
struct Layout {
    width: f64,
    outer: f64,
    inner: f64,
    hole: f64,
}

impl Layout {
    fn of(s: f64, two: bool, number: bool) -> Layout {
        let middle = s / 2.0;
        if number {
            // Thin, at the edge, the middle free for the digits.
            let width = (s * 0.09).round().max(1.0);
            let gap = (s * 0.04).round().max(1.0);
            let outer = middle - width / 2.0;
            let inner = outer - width - gap;
            let hole = if two { inner } else { outer } - width / 2.0;
            return Layout { width, outer, inner, hole };
        }
        // The rings' widths and places as shares of the icon, so it reads the
        // same at every size; a ring is never thinner than a pixel and a half.
        let width = (s * 0.16).max(1.5);
        let outer = middle - width / 2.0 - s * 0.02;
        let inner = outer - width * 1.35;
        Layout { width, outer, inner, hole: (if two { inner } else { outer }) - width / 2.0 }
    }
}

/// The largest whole scale at which a `w` × `h` box fits the hole of radius
/// `hole`, its corners within half a pixel of it; 1 where none does.
fn digit_scale(w: usize, h: usize, hole: f64) -> usize {
    let fits = |k: usize| {
        let (half_w, half_h) = ((w * k) as f64 / 2.0, (h * k) as f64 / 2.0);
        (half_w * half_w + half_h * half_h).sqrt() <= hole + 0.5
    };
    (1..=16).rev().find(|&k| fits(k)).unwrap_or(1)
}

/// The 5-hour window's color at `percent`, as the Mac app's.
pub fn five_hour_color(percent: f64) -> Rgb {
    if percent < 70.0 {
        Rgb(40, 180, 70)
    } else if percent < 90.0 {
        Rgb(255, 149, 0)
    } else {
        Rgb(255, 59, 48)
    }
}

/// The 7-day window's color at `percent`, as the Mac app's.
pub fn seven_day_color(percent: f64) -> Rgb {
    if percent < 70.0 {
        Rgb(192, 132, 252)
    } else if percent < 90.0 {
        Rgb(180, 80, 240)
    } else {
        Rgb(180, 30, 160)
    }
}

/// The tracks' grey and how much of it shows.
const TRACK: Rgb = Rgb(150, 150, 150);
const TRACK_ALPHA: f64 = 0.35;

/// Paints `color` at `alpha` over the straight RGBA pixel `px`.
fn over(px: &mut [u8], color: Rgb, alpha: f64) {
    if alpha <= 0.0 {
        return;
    }
    let below = f64::from(px[3]) / 255.0;
    let out = alpha + below * (1.0 - alpha);
    let mix = |top: u8, bottom: u8| ((f64::from(top) * alpha + f64::from(bottom) * below * (1.0 - alpha)) / out).round() as u8;
    px[0] = mix(color.0, px[0]);
    px[1] = mix(color.1, px[1]);
    px[2] = mix(color.2, px[2]);
    px[3] = (out * 255.0).round() as u8;
}

/// How much of a pixel at `r` from the middle lies within the band at
/// `radius`, `half` either side: a pixel's worth of smoothing.
fn band(r: f64, radius: f64, half: f64) -> f64 {
    (half - (r - radius).abs() + 0.5).clamp(0.0, 1.0)
}

/// How much of a pixel at angle `turn` (0–1, clockwise from the top), `r`
/// from the middle, lies within the first `fill` of the circle.
fn arc(turn: f64, r: f64, fill: f64) -> f64 {
    if fill >= 1.0 {
        return 1.0;
    }
    if fill <= 0.0 {
        return 0.0;
    }
    // How many pixels along the circle the pixel is from the arc's nearer
    // edge, its start (the top, 0 or 1) or its end (`fill`).
    let circumference = TAU * r.max(0.5);
    if turn <= fill {
        (turn.min(fill - turn) * circumference + 0.5).clamp(0.0, 1.0)
    } else {
        (0.5 - (turn - fill).min(1.0 - turn) * circumference).clamp(0.0, 1.0)
    }
}

impl Rings {
    /// The icon at `size` × `size` pixels.
    pub fn draw(&self, size: u32) -> Vec<u8> {
        let n = size as usize;
        let mut pixels = vec![0u8; n * n * 4];
        let s = f64::from(size);
        let middle = s / 2.0;
        let two = self.inner.is_some();
        let number = self.number.filter(|_| self.outer.is_some()).map(|n| n.min(100).to_string());
        let Layout { width, outer: outer_radius, inner: inner_radius, hole } = Layout::of(s, two, number.is_some());
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f64 + 0.5 - middle, y as f64 + 0.5 - middle);
                let r = (dx * dx + dy * dy).sqrt();
                // Clockwise from the top: y grows downward.
                let turn = (dx.atan2(-dy) / TAU).rem_euclid(1.0);
                let px = &mut pixels[(y * n + x) * 4..][..4];
                let mut ring = |radius: f64, ring: Option<Ring>| {
                    let cover = band(r, radius, width / 2.0);
                    if cover <= 0.0 {
                        return;
                    }
                    over(px, TRACK, cover * TRACK_ALPHA);
                    if let Some(ring) = ring {
                        over(px, ring.color, cover * arc(turn, r, ring.fill.clamp(0.0, 1.0)));
                    }
                };
                ring(outer_radius, self.outer);
                if two {
                    ring(inner_radius, self.inner);
                }
            }
        }
        if let (Some(text), Some(outer)) = (number, self.outer) {
            let (w, h) = (text_width(&text), DIGIT_HEIGHT);
            // One digit at the size two have, so it doesn't jump as the
            // number passes 9; 100 at whatever fits.
            let k = digit_scale(w.max(text_width("88")), h, hole);
            // Centred, on whole pixels.
            let left = n.saturating_sub(w * k) / 2;
            let top = n.saturating_sub(h * k) / 2;
            let mut x0 = left;
            for digit in text.bytes().map(|b| DIGITS[usize::from(b - b'0')]) {
                for (row, line) in digit.iter().enumerate() {
                    for (col, _) in line.bytes().enumerate().filter(|(_, b)| *b == b'#') {
                        for (dy, dx) in (0..k).flat_map(|dy| (0..k).map(move |dx| (dy, dx))) {
                            let (x, y) = (x0 + col * k + dx, top + row * k + dy);
                            if x < n && y < n {
                                pixels[(y * n + x) * 4..][..4].copy_from_slice(&[outer.color.0, outer.color.1, outer.color.2, 255]);
                            }
                        }
                    }
                }
                x0 += (digit[0].len() + DIGIT_GAP) * k;
            }
        }
        pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixels: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        pixels[((y * size + x) * 4) as usize..][..4].try_into().unwrap()
    }

    #[test]
    fn a_half_full_ring_colors_the_right_half_and_leaves_the_left_a_track() {
        let size = 64;
        let rings = Rings { outer: Some(Ring { fill: 0.5, color: Rgb(255, 0, 0) }), inner: None, number: None };
        let p = rings.draw(size);
        assert_eq!(p.len(), (size * size * 4) as usize);
        // On the outer ring's middle line, right and left of the middle.
        let s = f64::from(size);
        let width = s * 0.16;
        let at = (s / 2.0 + s / 2.0 - width / 2.0 - s * 0.02) as u32;
        let right = pixel(&p, size, at, size / 2);
        let left = pixel(&p, size, size - 1 - at, size / 2);
        assert_eq!(right, [255, 0, 0, 255], "filled, opaque");
        assert!(left[3] > 0 && left[3] < 140 && left[0] == left[1], "a faint grey track: {left:?}");
        // The middle and the corners are clear.
        assert_eq!(pixel(&p, size, size / 2, size / 2)[3], 0);
        assert_eq!(pixel(&p, size, 0, 0)[3], 0);
    }

    #[test]
    fn full_and_empty_rings_and_the_placeholder_draw_at_every_tray_size() {
        for size in [16, 22, 24, 32, 48, 64] {
            let full = Rings { outer: Some(Ring { fill: 1.0, color: Rgb(0, 200, 0) }), inner: Some(Ring { fill: 0.0, color: Rgb(0, 0, 200) }), number: None }.draw(size);
            let empty = Rings::default().draw(size);
            let green = full.chunks(4).filter(|p| p[1] > 150 && p[0] < 60 && p[3] > 200).count();
            assert!(green > (size * size / 10) as usize, "{size}: the outer ring all round");
            assert!(!full.chunks(4).any(|p| p[2] > 150 && p[3] > 200), "{size}: an empty inner ring shows no color");
            assert!(empty.chunks(4).any(|p| p[3] > 0), "{size}: the placeholder still shows its tracks");
            assert!(empty.chunks(4).all(|p| p[3] < 255), "{size}: and only faintly");
        }
    }

    /// The digits of a number drawn at `size`: each opaque pixel of the
    /// number's color, as rows of # and .
    fn digits_drawn(rings: &Rings, size: u32) -> Vec<String> {
        let color = rings.outer.unwrap().color;
        let p = rings.draw(size);
        let n = size as usize;
        let mut rows: Vec<String> = (0..n)
            .map(|y| (0..n).map(|x| if p[(y * n + x) * 4..][..4] == [color.0, color.1, color.2, 255] { '#' } else { '.' }).collect())
            .collect();
        // Only the middle, where the digits are (a full ring is the same
        // color), trimmed to the digits' own box.
        let (lo, hi) = (n / 5, n - n / 5);
        rows = rows[lo..hi].iter().map(|r| r[lo..hi].to_string()).filter(|r| r.contains('#')).collect();
        let first = rows.iter().filter_map(|r| r.find('#')).min().unwrap_or(0);
        let last = rows.iter().filter_map(|r| r.rfind('#')).max().unwrap_or(0);
        rows.iter().map(|r| r[first..=last].to_string()).collect()
    }

    #[test]
    fn the_number_is_drawn_sharp_in_the_middle_in_the_outer_rings_color() {
        let rings = Rings {
            outer: Some(Ring { fill: 0.9, color: Rgb(180, 30, 160) }),
            inner: Some(Ring { fill: 0.4, color: Rgb(40, 180, 70) }),
            number: Some(90),
        };
        // At 16 px, scale 1: "9", a gap, "0", every pixel opaque.
        assert_eq!(digits_drawn(&rings, 16), ["###.###", "#.#.#.#", "###.#.#", "..#.#.#", "###.###"]);
        // At 32 px, scale 2.
        let big = digits_drawn(&rings, 32);
        assert_eq!(big.len(), 10, "{big:#?}");
        assert_eq!(big[0], "######..######");
        // Centred within a pixel (inside the rings, which share its color).
        let p = rings.draw(16);
        let columns: Vec<usize> = (2..14).filter(|&x| (5..11).any(|y| p[(y * 16 + x) * 4..][..4] == [180, 30, 160, 255])).collect();
        assert!(columns.first() == Some(&4) && columns.last() == Some(&10), "{columns:?}");
        // None asked for, none drawn: nothing of the outer ring's color in
        // the middle, where the wide rings' inner one is.
        let none = Rings { number: None, ..rings }.draw(16);
        assert!((6..10).all(|y| (6..10).all(|x| none[(y * 16 + x) * 4..][..4] != [180, 30, 160, 255])));
        assert!(Rings { outer: None, inner: None, number: Some(5) }.draw(16).chunks(4).all(|p| p[3] < 255), "no ring, no number");
    }

    #[test]
    fn a_hundred_fits_and_the_digits_grow_with_the_icon() {
        let one = |number| Rings { outer: Some(Ring { fill: 1.0, color: Rgb(255, 59, 48) }), inner: None, number: Some(number) };
        assert_eq!(digits_drawn(&one(100), 16), [".#.###.###", "##.#.#.#.#", ".#.#.#.#.#", ".#.#.#.#.#", ".#.###.###"]);
        assert_eq!(digits_drawn(&one(7), 16), ["###", "..#", "..#", "..#", "..#"], "as big as two digits are");
        assert_eq!(digits_drawn(&one(250), 16), digits_drawn(&one(100), 16), "never past 100");
        // Two rings leave a smaller hole than one.
        let scale = |size: u32, two: bool| {
            let l = Layout::of(f64::from(size), two, true);
            digit_scale(text_width("90"), DIGIT_HEIGHT, l.hole)
        };
        assert_eq!([16, 22, 32, 64].map(|s| scale(s, true)), [1, 1, 2, 4]);
        assert_eq!([16, 22, 32, 64].map(|s| scale(s, false)), [1, 2, 3, 6]);
        // With a number the rings stay within the icon and clear of the digits.
        for size in [16u32, 22, 32, 64] {
            let l = Layout::of(f64::from(size), true, true);
            assert!(l.outer + l.width / 2.0 <= f64::from(size) / 2.0 + 1e-9 && l.inner - l.width / 2.0 >= l.hole - 1e-9, "{size}");
        }
    }

    /// With `U4AI_ICON_PICTURES=<dir>`, writes icons as raw RGBA
    /// (`<name>-<size>.rgba`), to look at them without a tray.
    #[test]
    fn icons_are_written_out_where_asked() {
        let Some(dir) = std::env::var_os("U4AI_ICON_PICTURES") else { return };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ring = |percent: f64, color: fn(f64) -> Rgb| Ring { fill: percent / 100.0, color: color(percent) };
        let cases = [
            ("both-42-76", Rings { outer: Some(ring(42.0, five_hour_color)), inner: Some(ring(76.0, seven_day_color)), number: Some(42) }),
            ("seven-day-90", Rings { outer: Some(ring(90.0, seven_day_color)), inner: None, number: Some(90) }),
            ("five-hour-100", Rings { outer: Some(ring(100.0, five_hour_color)), inner: Some(ring(55.0, seven_day_color)), number: Some(100) }),
            ("one-digit-7", Rings { outer: Some(ring(7.0, five_hour_color)), inner: Some(ring(3.0, seven_day_color)), number: Some(7) }),
            ("no-number-90", Rings { outer: Some(ring(90.0, seven_day_color)), inner: None, number: None }),
        ];
        for (name, rings) in cases {
            for size in [16, 22, 32, 64] {
                std::fs::write(dir.join(format!("{name}-{size}.rgba")), rings.draw(size)).unwrap();
            }
        }
    }

    #[test]
    fn the_colors_follow_the_mac_apps_thresholds() {
        assert_eq!(five_hour_color(69.9), Rgb(40, 180, 70));
        assert_eq!(five_hour_color(70.0), Rgb(255, 149, 0));
        assert_eq!(five_hour_color(90.0), Rgb(255, 59, 48));
        assert_eq!(seven_day_color(10.0), Rgb(192, 132, 252));
        assert_eq!(seven_day_color(95.0), Rgb(180, 30, 160));
    }
}
