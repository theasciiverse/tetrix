//! The nine TetriNET special blocks and what they do to a field.
//!
//! Rules follow TetriNET 1.13 as documented in its tetrinet.txt and as
//! implemented by tetrinet-0.11 (public domain). Where the original Windows
//! client is known to differ from tetrinet-0.11 (blockquake distances, add
//! line hole pattern, switch lowering the stack), the Windows behaviour is
//! the one modelled here; those cases are commented.

use super::field::{Field, Row, EMPTY, HEIGHT, SPECIAL_BASE, WIDTH};
use rand::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Special {
    AddLine,
    ClearLine,
    Nuke,
    RandomClear,
    Switch,
    ClearSpecials,
    Gravity,
    Quake,
    Bomb,
}

impl Special {
    /// Wire order `a c n r s b g q o`, also the frequency-table order.
    pub const ALL: [Special; 9] = [
        Special::AddLine,
        Special::ClearLine,
        Special::Nuke,
        Special::RandomClear,
        Special::Switch,
        Special::ClearSpecials,
        Special::Gravity,
        Special::Quake,
        Special::Bomb,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn letter(self) -> char {
        b"acnrsbgqo"[self.index()] as char
    }

    pub fn cell(self) -> u8 {
        SPECIAL_BASE + self.index() as u8
    }

    pub fn from_cell(c: u8) -> Option<Special> {
        c.checked_sub(SPECIAL_BASE).and_then(|i| Special::ALL.get(i as usize).copied())
    }

    pub fn name(self) -> &'static str {
        match self {
            Special::AddLine => "Add Line",
            Special::ClearLine => "Clear Line",
            Special::Nuke => "Nuke Field",
            Special::RandomClear => "Random Clear",
            Special::Switch => "Switch Fields",
            Special::ClearSpecials => "Clear Specials",
            Special::Gravity => "Gravity",
            Special::Quake => "Blockquake",
            Special::Bomb => "Block Bomb",
        }
    }

    /// Defensive specials ("GOOD blocks" in tetrinet.txt) are meant to be
    /// used on yourself.
    pub fn is_defensive(self) -> bool {
        matches!(self, Special::ClearLine | Special::Nuke | Special::Gravity)
    }
}

/// Something one player does to another player's field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attack {
    Special(Special),
    /// Classic-rules lines sent to everyone else on a multi-line clear.
    ClassicLines(u8),
}

/// Apply a single-field special (everything except Switch, which needs two
/// fields and is handled by the match). Returns false if the field topped out.
pub fn apply(field: &mut Field, special: Special, rng: &mut impl Rng) -> bool {
    match special {
        Special::AddLine => {
            // Windows TetriNET: each cell independently empty with chance 1/6,
            // plus one guaranteed hole, so a junk line is never full.
            let mut row: Row = [EMPTY; WIDTH];
            for c in row.iter_mut() {
                *c = rng.gen_range(0..=5);
            }
            row[rng.gen_range(0..WIDTH)] = EMPTY;
            return field.push_up(row);
        }
        Special::ClearLine => field.drop_bottom_row(),
        Special::Nuke => field.clear(),
        Special::RandomClear => {
            // "Removes 10 random blocks": ten random cells are emptied,
            // whether or not they held a block.
            for _ in 0..10 {
                field.set(rng.gen_range(0..WIDTH), rng.gen_range(0..HEIGHT), EMPTY);
            }
        }
        Special::Switch => {}
        Special::ClearSpecials => {
            for row in field.rows_mut() {
                for c in row.iter_mut() {
                    if *c >= SPECIAL_BASE {
                        *c = rng.gen_range(1..=5);
                    }
                }
            }
        }
        Special::Gravity => {
            for x in 0..WIDTH {
                let col: Vec<u8> = (0..HEIGHT).map(|y| field.get(x, y)).filter(|&c| c != EMPTY).collect();
                let pad = HEIGHT - col.len();
                for y in 0..HEIGHT {
                    field.set(x, y, if y < pad { EMPTY } else { col[y - pad] });
                }
            }
        }
        Special::Quake => {
            // Each row shifts 0..=3 cells left or right, blocks pushed past
            // the wall are lost. Distance odds 11:7:3:1 out of 22 for
            // 0/1/2/3, the commonly cited Windows TetriNET distribution.
            for row in field.rows_mut() {
                let roll = rng.gen_range(0..22);
                let dist = (roll < 1) as usize + (roll < 4) as usize + (roll < 11) as usize;
                if dist == 0 {
                    continue;
                }
                let old = *row;
                *row = [EMPTY; WIDTH];
                let right = rng.gen_bool(0.5);
                for x in 0..WIDTH {
                    let nx = if right { x as i32 + dist as i32 } else { x as i32 - dist as i32 };
                    if (0..WIDTH as i32).contains(&nx) {
                        row[nx as usize] = old[x];
                    }
                }
            }
        }
        Special::Bomb => {
            // Every 'o' block explodes: it vanishes and the 3x3 around it is
            // scattered at random over the lower 16 rows.
            let bomb = Special::Bomb.cell();
            let mut debris = Vec::new();
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    if field.get(x, y) != bomb {
                        continue;
                    }
                    field.set(x, y, EMPTY);
                    for ny in y.saturating_sub(1)..=(y + 1).min(HEIGHT - 1) {
                        for nx in x.saturating_sub(1)..=(x + 1).min(WIDTH - 1) {
                            let c = field.get(nx, ny);
                            if c != EMPTY && c != bomb {
                                debris.push(c);
                                field.set(nx, ny, EMPTY);
                            }
                        }
                    }
                }
            }
            for c in debris {
                field.set(rng.gen_range(0..WIDTH), rng.gen_range(HEIGHT - 16..HEIGHT), c);
            }
        }
    }
    true
}

/// A classic-rules junk line: solid random colours with one hole.
pub fn classic_line(rng: &mut impl Rng) -> Row {
    let mut row: Row = [EMPTY; WIDTH];
    for c in row.iter_mut() {
        *c = rng.gen_range(1..=5);
    }
    row[rng.gen_range(0..WIDTH)] = EMPTY;
    row
}

/// After a field switch, the stack is lowered so its top six rows are empty
/// ("If either of the fields' block stack is over 16 blocks high, the stack
/// will be lowered", tetrinet.txt).
pub fn lower_after_switch(field: &mut Field) {
    let h = field.stack_height();
    if h > HEIGHT - 6 {
        field.shift_down(h - (HEIGHT - 6));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::SmallRng, SeedableRng};

    #[test]
    fn letters_round_trip() {
        for s in Special::ALL {
            assert_eq!(Special::from_cell(s.cell()), Some(s));
        }
        assert_eq!(Special::from_cell(5), None);
    }

    #[test]
    fn gravity_compacts_columns() {
        let mut f = Field::default();
        f.set(3, 0, 1);
        f.set(3, 10, 2);
        apply(&mut f, Special::Gravity, &mut SmallRng::seed_from_u64(1));
        assert_eq!(f.get(3, HEIGHT - 1), 2);
        assert_eq!(f.get(3, HEIGHT - 2), 1);
        assert_eq!(f.stack_height(), 2);
    }

    #[test]
    fn add_line_never_full() {
        let mut rng = SmallRng::seed_from_u64(7);
        for _ in 0..200 {
            let mut f = Field::default();
            apply(&mut f, Special::AddLine, &mut rng);
            assert!(f.rows()[HEIGHT - 1].contains(&EMPTY));
        }
    }
}
