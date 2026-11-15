use crate::errors::{ErrorCode, PeleError, Result};

#[derive(Debug, Clone, Default)]
pub struct Cell {
    // Indices into the flat view/cell slice.
    pub children: Vec<usize>,
    pub columns: i64,
    pub span: usize,
    pub space: bool,
    pub head: f64,
    pub nw: f64,
    pub nh: f64,
    pub col: usize,
    pub row: usize,
    pub cols: usize,
    pub heights: Vec<f64>,
    pub row_gaps: Vec<f64>,
    pub col_gaps: Vec<f64>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

const MAX_SPAN: usize = 1000;

pub fn calculate_block_position(columns: i64, position: usize) -> Result<(usize, usize)> {
    if columns == 0 {
        return Err(PeleError::new("Columns must be an integer !== 0.", ErrorCode::Semantic).of("block"));
    }
    if columns < 0 {
        return Ok((position, 0));
    }
    let cols = columns as usize;
    Ok((position % cols, position / cols))
}

fn total(gaps: &[f64]) -> f64 {
    gaps.iter().copied().sum()
}

fn measure(cells: &mut [Cell], idx: usize, gap: f64, inset: f64) -> Result<()> {
    let columns = cells[idx].columns;
    let children = cells[idx].children.clone();
    let mut heights: Vec<f64> = Vec::new();
    let mut position = 0usize;
    let mut cols = 0usize;
    let mut cw = 0.0f64;
    let mut tallest = 0.0f64;

    for &c_idx in &children {
        let (px, py) = calculate_block_position(columns, position)?;
        let raw_span = cells[c_idx].span;
        let mut span = if raw_span >= 1 { raw_span.min(MAX_SPAN) } else { 1 };
        if columns > 0 {
            let max_cols = columns as usize;
            if span > max_cols - px {
                span = max_cols - px;
            }
        }
        cells[c_idx].span = span;
        cells[c_idx].col = px;
        cells[c_idx].row = py;
        position += span;
        if px + span > cols {
            cols = px + span;
        }
        if py == heights.len() {
            heights.push(0.0);
        }
        if cells[c_idx].children.is_empty() {
            heights[py] = -1.0;
        } else if heights[py] >= 0.0 && cells[c_idx].nh > heights[py] {
            heights[py] = cells[c_idx].nh;
        }
        if cells[c_idx].space {
            continue;
        }
        let width = (cells[c_idx].nw - gap * ((span - 1) as f64)) / (span as f64);
        if width > cw {
            cw = width;
        }
        if cells[c_idx].nh > tallest {
            tallest = cells[c_idx].nh;
        }
    }

    let mut height = ((heights.len() - 1) as f64) * gap + total(&cells[idx].row_gaps);
    for h in &mut heights {
        if *h < 0.0 {
            *h = tallest;
        }
        height += *h;
    }
    let col_gaps_sum = total(&cells[idx].col_gaps);
    let head = cells[idx].head;
    cells[idx].cols = cols;
    cells[idx].heights = heights;
    cells[idx].nw = cells[idx]
        .nw
        .max((cols as f64) * cw + ((cols - 1) as f64) * gap + col_gaps_sum + 2.0 * inset);
    cells[idx].nh = height + 2.0 * inset + head;
    Ok(())
}

fn arrange(cells: &mut [Cell], idx: usize, gap: f64, inset: f64) {
    let cols = cells[idx].cols;
    let heights = cells[idx].heights.clone();
    let row_gaps = cells[idx].row_gaps.clone();
    let col_gaps = cells[idx].col_gaps.clone();
    let children = cells[idx].children.clone();
    let box_w = cells[idx].w;
    let box_h = cells[idx].h;
    let box_x = cells[idx].x;
    let box_y = cells[idx].y;
    let box_head = cells[idx].head;

    let rows = heights.len();
    let cw = (box_w - 2.0 * inset - ((cols - 1) as f64) * gap - total(&col_gaps)) / (cols as f64);
    let room = box_h - 2.0 * inset - box_head - ((rows - 1) as f64) * gap - total(&row_gaps);
    let mut offsets: Vec<f64> = Vec::new();
    if !col_gaps.is_empty() {
        let mut sum = 0.0f64;
        for col in 0..cols {
            offsets.push(sum);
            sum += col_gaps.get(col).copied().unwrap_or(0.0);
        }
    }
    let needed: f64 = heights.iter().copied().sum();
    let scale = if needed > 0.0 { room / needed } else { 0.0 };
    let share = if needed > 0.0 { 0.0 } else { room / (rows as f64) };
    let left = box_x - box_w / 2.0 + inset;
    let mut top = box_y - box_h / 2.0 + inset + box_head;
    let mut row = 0usize;

    for &c_idx in &children {
        let c_row = cells[c_idx].row;
        while row < c_row {
            top += heights[row] * scale + share + gap + row_gaps.get(row).copied().unwrap_or(0.0);
            row += 1;
        }
        let c_col = cells[c_idx].col;
        let c_span = cells[c_idx].span;
        let before = offsets.get(c_col).copied().unwrap_or(0.0);
        let after = offsets.get(c_col + c_span - 1).copied().unwrap_or(0.0);
        let child_w = cw * (c_span as f64) + gap * ((c_span - 1) as f64) + after - before;
        let child_h = heights[row] * scale + share;
        cells[c_idx].w = child_w;
        cells[c_idx].h = child_h;
        cells[c_idx].x = left + (c_col as f64) * (cw + gap) + before + child_w / 2.0;
        cells[c_idx].y = top + child_h / 2.0;
    }
}

// Lays out a BFS-ordered slice of cells in place (`cells[0]` is root).
pub fn layout_cells(cells: &mut [Cell], gap: f64) -> Result<(f64, f64)> {
    for i in (0..cells.len()).rev() {
        if !cells[i].children.is_empty() {
            measure(cells, i, gap, if i == 0 { 0.0 } else { gap })?;
        }
    }
    cells[0].w = cells[0].nw;
    cells[0].h = cells[0].nh;
    cells[0].x = cells[0].w / 2.0;
    cells[0].y = cells[0].h / 2.0;
    for i in 0..cells.len() {
        if !cells[i].children.is_empty() {
            arrange(cells, i, gap, if i == 0 { 0.0 } else { gap });
        }
    }
    Ok((cells[0].w, cells[0].h))
}
