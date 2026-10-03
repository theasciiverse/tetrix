//! The seven TetriNET pieces and their rotation tables.
//!
//! The shape table is ported verbatim from Andrew Church's tetrinet-0.11
//! (public domain), so rotation behaves exactly like classic TetriNET:
//! every piece rotates about a fixed "hot spot" cell, with no SRS-style
//! wall-kick tables.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Bar,
    Square,
    ReverseL,
    L,
    Z,
    S,
    T,
}

impl PieceKind {
    /// TetriNET order, which is also the order of the `pieces` frequency list.
    pub const ALL: [PieceKind; 7] = [
        PieceKind::Bar,
        PieceKind::Square,
        PieceKind::ReverseL,
        PieceKind::L,
        PieceKind::Z,
        PieceKind::S,
        PieceKind::T,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// Block colour 1..=5, the same `piece % 5 + 1` mapping TetriNET uses
    /// (so S shares blue with the bar and T shares yellow with the square).
    pub fn color(self) -> u8 {
        (self.index() % 5) as u8 + 1
    }

    pub fn data(self, rot: usize) -> &'static PieceData {
        &table()[self.index()][rot % 4]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PieceData {
    pub hot_x: i32,
    pub hot_y: i32,
    pub top: i32,
    pub left: i32,
    pub bottom: i32,
    pub right: i32,
    pub cells: [[bool; 4]; 4],
}

impl PieceData {
    /// Absolute field coordinates of each block with the hot spot at (x, y).
    pub fn blocks(&self, x: i32, y: i32) -> impl Iterator<Item = (i32, i32)> + '_ {
        let ox = x - self.hot_x;
        let oy = y - self.hot_y;
        (0..4).flat_map(move |j| {
            (0..4).filter_map(move |i| self.cells[j][i].then_some((ox + i as i32, oy + j as i32)))
        })
    }
}

/// `SHAPES[piece][row][rotation]`, rotations ordered clockwise.
/// `#` is a block, `X` is the hot spot (a block), `x` a hot spot on an
/// empty cell, `.` empty.
const SHAPES: [[[&str; 4]; 4]; 7] = [
    [
        ["##X#", "..X.", "##X#", "..X."],
        ["....", "..#.", "....", "..#."],
        ["....", "..#.", "....", "..#."],
        ["....", "..#.", "....", "..#."],
    ],
    [
        ["....", "....", "....", "...."],
        [".X#.", ".X#.", ".X#.", ".X#."],
        [".##.", ".##.", ".##.", ".##."],
        ["....", "....", "....", "...."],
    ],
    [
        ["....", ".#..", "#...", ".##."],
        ["#X#.", ".X..", "#X#.", ".X.."],
        ["..#.", "##..", "....", ".#.."],
        ["....", "....", "....", "...."],
    ],
    [
        ["....", "##..", "..#.", ".#.."],
        ["#X#.", ".X..", "#X#.", ".X.."],
        ["#...", ".#..", "....", ".##."],
        ["....", "....", "....", "...."],
    ],
    [
        ["....", ".#..", "....", ".#.."],
        ["#X..", "#X..", "#X..", "#X.."],
        [".##.", "#...", ".##.", "#..."],
        ["....", "....", "....", "...."],
    ],
    [
        ["....", "#...", "....", "#..."],
        [".X#.", "#X..", ".X#.", "#X.."],
        ["##..", ".#..", "##..", ".#.."],
        ["....", "....", "....", "...."],
    ],
    [
        ["....", ".#..", ".#..", ".#.."],
        ["#X#.", "#X..", "#X#.", ".X#."],
        [".#..", ".#..", "....", ".#.."],
        ["....", "....", "....", "...."],
    ],
];

fn table() -> &'static [[PieceData; 4]; 7] {
    static TABLE: OnceLock<[[PieceData; 4]; 7]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let blank = PieceData {
            hot_x: -1,
            hot_y: -1,
            top: 3,
            left: 3,
            bottom: 0,
            right: 0,
            cells: [[false; 4]; 4],
        };
        let mut t = [[blank; 4]; 7];
        for (p, rows) in SHAPES.iter().enumerate() {
            for r in 0..4 {
                let d = &mut t[p][r];
                for (y, row) in rows.iter().enumerate() {
                    for (x, ch) in row[r].bytes().enumerate() {
                        let (x, y) = (x as i32, y as i32);
                        if matches!(ch, b'x' | b'X') {
                            d.hot_x = x;
                            d.hot_y = y;
                        }
                        if matches!(ch, b'#' | b'X') {
                            d.cells[y as usize][x as usize] = true;
                            d.top = d.top.min(y);
                            d.left = d.left.min(x);
                            d.bottom = d.bottom.max(y);
                            d.right = d.right.max(x);
                        }
                    }
                }
                assert!(d.hot_x >= 0, "piece {p} rotation {r} missing hot spot");
            }
        }
        t
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rotation_has_four_blocks() {
        for kind in PieceKind::ALL {
            for rot in 0..4 {
                assert_eq!(kind.data(rot).blocks(0, 0).count(), 4, "{kind:?} rot {rot}");
            }
        }
    }
}
