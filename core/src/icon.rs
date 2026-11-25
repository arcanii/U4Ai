//! The tray's icon, drawn in code at whatever size the tray asks: the Mac
//! app's rings, the 5-hour window outside (green, then orange from 70%,
//! red from 90%) and the 7-day inside (light purple, deep purple, magenta),
//! each over a faint track. Without a 5-hour window the 7-day takes the
//! outer ring alone; without any usage both are tracks. Each pixel's
//! coverage is worked out from its distance to the ring and its angle, so
//! the edges are smooth at 16 pixels as at 64. Straight (not
//! premultiplied) RGBA, rows top to bottom.

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
/// is the placeholder (tracks alone).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rings {
    pub outer: Option<Ring>,
    pub inner: Option<Ring>,
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
        // The rings' widths and places as shares of the icon, so it reads the
        // same at every size; a ring is never thinner than a pixel and a half.
        let width = (s * 0.16).max(1.5);
        let outer_radius = middle - width / 2.0 - s * 0.02;
        let inner_radius = outer_radius - width * 1.35;
        let two = self.inner.is_some();
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
        let rings = Rings { outer: Some(Ring { fill: 0.5, color: Rgb(255, 0, 0) }), inner: None };
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
            let full = Rings { outer: Some(Ring { fill: 1.0, color: Rgb(0, 200, 0) }), inner: Some(Ring { fill: 0.0, color: Rgb(0, 0, 200) }) }.draw(size);
            let empty = Rings::default().draw(size);
            let green = full.chunks(4).filter(|p| p[1] > 150 && p[0] < 60 && p[3] > 200).count();
            assert!(green > (size * size / 10) as usize, "{size}: the outer ring all round");
            assert!(!full.chunks(4).any(|p| p[2] > 150 && p[3] > 200), "{size}: an empty inner ring shows no color");
            assert!(empty.chunks(4).any(|p| p[3] > 0), "{size}: the placeholder still shows its tracks");
            assert!(empty.chunks(4).all(|p| p[3] < 255), "{size}: and only faintly");
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
