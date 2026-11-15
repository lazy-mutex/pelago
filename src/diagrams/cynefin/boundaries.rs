use crate::svg::builder::num;

fn to_u32(x: f64) -> u32 {
    if !x.is_finite() || x == 0.0 {
        return 0;
    }
    x.trunc().rem_euclid(4294967296.0) as u32
}

// mulberry32, one step.
pub fn seeded_random(seed: f64) -> f64 {
    let mut t = to_u32(seed + 0x6d2b79f5u32 as f64);
    t = (t ^ (t >> 15)).wrapping_mul(t | 1);
    t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
    ((t ^ (t >> 14)) as f64) / 4294967296.0
}

pub fn hash_string(s: &str) -> i32 {
    let mut hash: i32 = 0;
    for code in s.encode_utf16() {
        hash = (hash << 5).wrapping_sub(hash).wrapping_add(code as i32);
    }
    hash
}

pub fn resolve_seed(configured: Option<f64>, id: &str) -> f64 {
    match configured {
        Some(c) if c.is_finite() && c != 0.0 => c,
        _ => hash_string(id) as f64,
    }
}

#[derive(Clone, Copy)]
pub struct WaveOptions {
    pub segments: usize,
    pub vertical: bool,
    pub point_step: f64,
    pub control_step: f64,
    pub control_shift: f64,
    pub pin_start: bool,
    pub pin_end: bool,
}

pub const FOLD: WaveOptions = WaveOptions {
    segments: 7,
    vertical: true,
    point_step: 17.0,
    control_step: 31.0,
    control_shift: 7.0,
    pin_start: false,
    pin_end: false,
};

pub const HORIZON: WaveOptions = WaveOptions {
    segments: 7,
    vertical: false,
    point_step: 23.0,
    control_step: 37.0,
    control_shift: 11.0,
    pin_start: false,
    pin_end: false,
};

pub fn wave(from: f64, to: f64, center: f64, seed: f64, amplitude: f64, o: WaveOptions) -> Vec<f64> {
    let step = (to - from) / o.segments as f64;
    let mut along = Vec::with_capacity(o.segments + 1);
    let mut across = Vec::with_capacity(o.segments + 1);
    for i in 0..=o.segments {
        let pinned = (i == 0 && o.pin_start) || (i == o.segments && o.pin_end);
        along.push(from + i as f64 * step);
        across.push(if pinned {
            center
        } else {
            center + seeded_random(seed + i as f64 * o.point_step) * amplitude * 2.0 - amplitude
        });
    }
    let mut out = Vec::with_capacity(2 + o.segments * 6);
    if o.vertical {
        out.extend([across[0], along[0]]);
    } else {
        out.extend([along[0], across[0]]);
    }
    for i in 0..o.segments {
        let mid = (along[i] + along[i + 1]) / 2.0;
        let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
        let offset = amplitude * 1.5 * dir * seeded_random(seed + i as f64 * o.control_step + o.control_shift);
        if o.vertical {
            out.extend([
                across[i] + offset,
                mid,
                across[i + 1] - offset,
                mid,
                across[i + 1],
                along[i + 1],
            ]);
        } else {
            out.extend([
                mid,
                across[i] + offset,
                mid,
                across[i + 1] - offset,
                along[i + 1],
                across[i + 1],
            ]);
        }
    }
    out
}

pub fn cliff(cx: f64, top: f64, bottom: f64, amplitude: f64) -> Vec<f64> {
    let h = bottom - top;
    vec![
        cx,
        top,
        cx + amplitude,
        top + h * 0.2,
        cx - amplitude * 1.5,
        top + h * 0.55,
        cx + amplitude * 0.5,
        top + h * 0.75,
        cx - amplitude,
        top + h * 0.85,
        cx + amplitude * 0.3,
        top + h * 0.95,
        cx,
        bottom,
    ]
}

pub fn forward(curve: &[f64]) -> String {
    let mut d = String::new();
    let mut i = 2usize;
    while i + 5 < curve.len() {
        d.push_str(&format!(
            "C{},{} {},{} {},{}",
            num(curve[i]),
            num(curve[i + 1]),
            num(curve[i + 2]),
            num(curve[i + 3]),
            num(curve[i + 4]),
            num(curve[i + 5])
        ));
        i += 6;
    }
    d
}

pub fn backward(curve: &[f64]) -> String {
    let mut d = String::new();
    let mut i = curve.len();
    while i >= 8 {
        i -= 6;
        d.push_str(&format!(
            "C{},{} {},{} {},{}",
            num(curve[i + 2]),
            num(curve[i + 3]),
            num(curve[i]),
            num(curve[i + 1]),
            num(curve[i - 2]),
            num(curve[i - 1])
        ));
    }
    d
}

pub fn curve_path(curve: &[f64]) -> String {
    format!("M{},{}{}", num(curve[0]), num(curve[1]), forward(curve))
}

pub fn ellipse_path(cx: f64, cy: f64, rx: f64, ry: f64) -> String {
    let r = format!("{},{}", num(rx), num(ry));
    format!(
        "M{},{}A{} 0 1,1 {},{}A{} 0 1,1 {},{}Z",
        num(cx - rx),
        num(cy),
        r,
        num(cx + rx),
        num(cy),
        r,
        num(cx - rx),
        num(cy)
    )
}
