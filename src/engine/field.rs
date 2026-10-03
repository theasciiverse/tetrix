//! The 12x22 TetriNET playing field.
//!
//! Cell values follow the TetriNET wire encoding so the field can later be
//! sent over the network unchanged: 0 empty, 1..=5 coloured blocks,
//! 6..=14 special blocks (`a c n r s b g q o` in that order).

use super::piece::PieceKind;

pub const WIDTH: usize = 12;
pub const HEIGHT: usize = 22;
pub const EMPTY: u8 = 0;
pub const SPECIAL_BASE: u8 = 6;

pub type Row = [u8; WIDTH];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    rows: [Row; HEIGHT],
}

impl Default for Field {
    fn default() -> Self {
        Field { rows: [[EMPTY; WIDTH]; HEIGHT] }
    }
}

impl Field {
    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.rows[y][x]
    }

    pub fn set(&mut self, x: usize, y: usize, v: u8) {
        self.rows[y][x] = v;
    }

    pub fn rows(&self) -> &[Row; HEIGHT] {
        &self.rows
    }

    pub fn rows_mut(&mut self) -> &mut [Row; HEIGHT] {
        &mut self.rows
    }

    /// Whether `kind` at rotation `rot` with its hot spot at (x, y) fits.
    /// Blocks above the top edge are allowed, as in TetriNET.
    pub fn fits(&self, kind: PieceKind, rot: usize, x: i32, y: i32) -> bool {
        kind.data(rot).blocks(x, y).all(|(bx, by)| {
            bx >= 0
                && bx < WIDTH as i32
                && by < HEIGHT as i32
                && (by < 0 || self.rows[by as usize][bx as usize] == EMPTY)
        })
    }

    /// Remove every full row, shifting the rows above down. Returns the
    /// cleared rows (original index and contents), top to bottom.
    pub fn clear_full_rows(&mut self) -> Vec<(usize, Row)> {
        let cleared: Vec<(usize, Row)> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.iter().all(|&c| c != EMPTY))
            .map(|(y, r)| (y, *r))
            .collect();
        if !cleared.is_empty() {
            let kept: Vec<Row> = self.rows.iter().filter(|r| r.iter().any(|&c| c == EMPTY)).copied().collect();
            let pad = HEIGHT - kept.len();
            for y in 0..HEIGHT {
                self.rows[y] = if y < pad { [EMPTY; WIDTH] } else { kept[y - pad] };
            }
        }
        cleared
    }

    /// Push the whole field up one row and insert `row` at the bottom.
    /// Returns false when that shoves blocks off the top (the player tops out).
    pub fn push_up(&mut self, row: Row) -> bool {
        let ok = self.rows[0].iter().all(|&c| c == EMPTY);
        self.rows.copy_within(1.., 0);
        self.rows[HEIGHT - 1] = row;
        ok
    }

    /// Drop the bottom row and shift everything down one.
    pub fn drop_bottom_row(&mut self) {
        self.rows.copy_within(..HEIGHT - 1, 1);
        self.rows[0] = [EMPTY; WIDTH];
    }

    /// Shift everything down `n` rows; rows falling off the bottom are lost.
    pub fn shift_down(&mut self, n: usize) {
        for _ in 0..n.min(HEIGHT) {
            self.drop_bottom_row();
        }
    }

    /// Height of the stack: rows from the bottom up to the highest block.
    pub fn stack_height(&self) -> usize {
        self.rows
            .iter()
            .position(|r| r.iter().any(|&c| c != EMPTY))
            .map_or(0, |top| HEIGHT - top)
    }

    pub fn column_heights(&self) -> [usize; WIDTH] {
        let mut h = [0; WIDTH];
        for (x, hx) in h.iter_mut().enumerate() {
            *hx = (0..HEIGHT).find(|&y| self.rows[y][x] != EMPTY).map_or(0, |y| HEIGHT - y);
        }
        h
    }

    pub fn clear(&mut self) {
        *self = Field::default();
    }
}
