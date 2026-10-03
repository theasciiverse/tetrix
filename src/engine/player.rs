//! One player's game: field, falling piece, level, special inventory.
//!
//! Pure simulation with no terminal or network code. Time advances through
//! `tick(dt_ms)`; everything that other systems may care about (locks, line
//! clears, classic lines to send, losing) is pushed to `events` for the
//! match, the renderer's effects, and later the network layer to drain.

use super::field::{Field, Row, HEIGHT, SPECIAL_BASE, WIDTH};
use super::piece::PieceKind;
use super::settings::{level_delay, pick_weighted, Settings};
use super::special::{self, Attack, Special};
use rand::{rngs::SmallRng, Rng, SeedableRng};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Active {
    pub kind: PieceKind,
    pub rot: usize,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug)]
pub enum Event {
    Locked { cells: Vec<(i32, i32)>, color: u8 },
    LinesCleared { rows: Vec<(usize, Row)>, specials_gained: usize },
    /// Classic-rules junk lines this player sends to everyone else.
    SendLines(u8),
    LevelUp(u32),
    /// The field changed in a way other players should see (network sync).
    FieldChanged,
    Lost,
}

pub struct Player {
    pub settings: Settings,
    pub field: Field,
    pub piece: Option<Active>,
    pub next: PieceKind,
    pub level: u32,
    /// Level used for fall speed; differs from `level` when levels are averaged.
    pub speed_level: u32,
    pub lines: u32,
    pub inventory: VecDeque<Special>,
    pub alive: bool,
    /// Increments with every spawn, so observers can tell pieces apart.
    pub piece_serial: u64,
    pub events: Vec<Event>,
    last_special: u32,
    waiting: bool,
    timer: i64,
    /// Shared seed across players gives everyone the same piece sequence
    /// (TetriNET 1.14's "same blocks" option).
    piece_rng: SmallRng,
    rng: SmallRng,
}

impl Player {
    pub fn new(settings: Settings, piece_seed: u64, seed: u64) -> Self {
        let mut piece_rng = SmallRng::seed_from_u64(piece_seed);
        let next = Self::roll_piece(&settings, &mut piece_rng);
        Player {
            level: settings.initial_level,
            speed_level: settings.initial_level,
            settings,
            field: Field::default(),
            piece: None,
            next,
            lines: 0,
            inventory: VecDeque::new(),
            alive: true,
            piece_serial: 0,
            events: Vec::new(),
            last_special: 0,
            // tetrinet-0.11 waits 1.2 s before the first piece of a game.
            waiting: true,
            timer: 1200,
            piece_rng,
            rng: SmallRng::seed_from_u64(seed),
        }
    }

    fn roll_piece(settings: &Settings, rng: &mut SmallRng) -> PieceKind {
        PieceKind::ALL[pick_weighted(&settings.piece_freq, rng.gen_range(0..100))]
    }

    pub fn tick(&mut self, dt_ms: i64) {
        if !self.alive {
            return;
        }
        self.timer -= dt_ms;
        if self.timer <= 0 {
            if self.waiting {
                self.spawn();
            } else {
                self.step_down();
            }
        }
    }

    fn gravity_delay(&self) -> i64 {
        level_delay(self.speed_level)
    }

    fn spawn(&mut self) {
        let kind = self.next;
        self.next = Self::roll_piece(&self.settings, &mut self.piece_rng);
        let d = kind.data(0);
        let y = d.hot_y - d.top;
        self.waiting = false;
        self.timer = self.gravity_delay();
        // Try the spawn column, then one left, then one right, as TetriNET does.
        for x in [6, 5, 7] {
            if self.field.fits(kind, 0, x, y) {
                self.piece = Some(Active { kind, rot: 0, x, y });
                self.piece_serial += 1;
                return;
            }
        }
        self.top_out();
    }

    pub fn ghost_y(&self) -> Option<i32> {
        let p = self.piece?;
        let mut y = p.y;
        while self.field.fits(p.kind, p.rot, p.x, y + 1) {
            y += 1;
        }
        Some(y)
    }

    pub fn shift(&mut self, dx: i32) -> bool {
        let Some(p) = self.piece.as_mut() else { return false };
        if self.field.fits(p.kind, p.rot, p.x + dx, p.y) {
            p.x += dx;
            true
        } else {
            false
        }
    }

    /// Rotate clockwise (`dir = 1`) or counter-clockwise (`dir = -1`). A
    /// rotation blocked only by a wall nudges the piece back inside the
    /// field; one blocked by other blocks fails.
    pub fn rotate(&mut self, dir: i32) -> bool {
        let Some(p) = self.piece.as_mut() else { return false };
        let rot = (p.rot as i32 + dir).rem_euclid(4) as usize;
        let d = p.kind.data(rot);
        let left = p.x - d.hot_x + d.left;
        let right = p.x - d.hot_x + d.right;
        let nudge = if left < 0 { -left } else if right >= WIDTH as i32 { WIDTH as i32 - 1 - right } else { 0 };
        if self.field.fits(p.kind, rot, p.x + nudge, p.y) {
            p.rot = rot;
            p.x += nudge;
            true
        } else {
            false
        }
    }

    /// One row down (the Down key, and also what gravity does).
    pub fn step_down(&mut self) {
        let Some(p) = self.piece.as_mut() else { return };
        if self.field.fits(p.kind, p.rot, p.x, p.y + 1) {
            p.y += 1;
            self.timer = self.gravity_delay();
        } else {
            self.lock();
        }
    }

    pub fn hard_drop(&mut self) {
        let Some(y) = self.ghost_y() else { return };
        if let Some(p) = self.piece.as_mut() {
            p.y = y;
        }
        if self.settings.slide {
            self.timer = self.gravity_delay();
        } else {
            self.lock();
        }
    }

    fn lock(&mut self) {
        let Some(p) = self.piece.take() else { return };
        let color = p.kind.color();
        let cells: Vec<(i32, i32)> = p.kind.data(p.rot).blocks(p.x, p.y).collect();
        for &(x, y) in &cells {
            if y >= 0 {
                self.field.set(x as usize, y as usize, color);
            }
        }
        self.events.push(Event::Locked { cells, color });

        let cleared = self.field.clear_full_rows();
        let count = cleared.len() as u32;
        let gained = if count > 0 { self.collect_specials(&cleared) } else { 0 };
        if count > 0 {
            self.events.push(Event::LinesCleared { rows: cleared, specials_gained: gained });
        }
        self.lines += count;

        if self.settings.classic && count > 1 {
            self.events.push(Event::SendLines(if count < 4 { count as u8 - 1 } else { 4 }));
        }

        let s = &self.settings;
        let level = (s.initial_level + (self.lines / s.lines_per_level.max(1)) * s.level_inc).min(100);
        if level != self.level {
            self.level = level;
            self.events.push(Event::LevelUp(level));
        }

        let n = (self.lines - self.last_special) / self.settings.special_lines.max(1);
        self.last_special += n * self.settings.special_lines.max(1);
        self.place_specials((n * self.settings.special_count) as usize);

        self.events.push(Event::FieldChanged);
        self.waiting = true;
        self.timer = self.settings.spawn_delay_ms;
    }

    /// Specials in cleared rows go to the inventory, once per line cleared
    /// ("the more lines you clear at the same time ... the more special
    /// blocks", tetrinet.txt), each landing at the front or back at random
    /// (TetriNET 1.1a: "special blocks you get are now randomly placed").
    fn collect_specials(&mut self, cleared: &[(usize, Row)]) -> usize {
        let found: Vec<Special> =
            cleared.iter().flat_map(|(_, r)| r.iter().filter_map(|&c| Special::from_cell(c))).collect();
        let mut gained = 0;
        for _ in 0..cleared.len() {
            for &s in &found {
                if self.inventory.len() >= self.settings.special_capacity {
                    return gained;
                }
                if self.rng.gen_bool(0.5) {
                    self.inventory.push_front(s);
                } else {
                    self.inventory.push_back(s);
                }
                gained += 1;
            }
        }
        gained
    }

    /// Turn `num` random plain blocks in the field into specials.
    fn place_specials(&mut self, num: usize) {
        let mut spots: Vec<(usize, usize)> = (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .filter(|&(x, y)| (1..SPECIAL_BASE).contains(&self.field.get(x, y)))
            .collect();
        for _ in 0..num.min(spots.len()) {
            let (x, y) = spots.swap_remove(self.rng.gen_range(0..spots.len()));
            let s = Special::ALL[pick_weighted(&self.settings.special_freq, self.rng.gen_range(0..100))];
            self.field.set(x, y, s.cell());
        }
    }

    /// Take an attack from another player (or a defensive special from self).
    pub fn receive(&mut self, attack: Attack) {
        if !self.alive {
            return;
        }
        let ok = match attack {
            Attack::Special(s) => special::apply(&mut self.field, s, &mut self.rng),
            Attack::ClassicLines(n) => {
                (0..n).all(|_| {
                    let row = special::classic_line(&mut self.rng);
                    self.field.push_up(row)
                })
            }
        };
        if !ok {
            self.top_out();
            return;
        }
        self.after_field_change();
    }

    /// Clean up after the field was changed from outside: clear any lines
    /// that became full (specials in them are not collected) and push the
    /// falling piece up out of any blocks now overlapping it.
    pub fn after_field_change(&mut self) {
        self.field.clear_full_rows();
        if let Some(p) = self.piece.as_mut() {
            while !self.field.fits(p.kind, p.rot, p.x, p.y) && p.y > -4 {
                p.y -= 1;
            }
        }
        self.events.push(Event::FieldChanged);
    }

    pub fn discard_special(&mut self) -> Option<Special> {
        self.inventory.pop_front()
    }

    /// The player loses: the field fills with random blocks, as in TetriNET.
    pub fn top_out(&mut self) {
        if !self.alive {
            return;
        }
        self.alive = false;
        self.piece = None;
        for row in self.field.rows_mut() {
            for c in row.iter_mut() {
                *c = self.rng.gen_range(1..=5);
            }
        }
        self.events.push(Event::Lost);
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_one_is_one_second() {
        assert_eq!(level_delay(1), 1000);
        assert!(level_delay(100) < level_delay(50));
    }

    #[test]
    fn hard_drop_locks_and_respawns() {
        let mut p = Player::new(Settings::default(), 1, 2);
        p.tick(1200);
        assert!(p.piece.is_some());
        p.hard_drop();
        assert!(p.piece.is_none());
        p.tick(1);
        assert!(p.piece.is_some());
        assert!(p.field.stack_height() > 0);
    }

    #[test]
    fn four_lines_send_four() {
        let mut p = Player::new(Settings::default(), 1, 2);
        for y in HEIGHT - 4..HEIGHT {
            for x in 0..WIDTH {
                if x != 0 {
                    p.field.set(x, y, 1);
                }
            }
        }
        // Bar rotation 1 is vertical; put its column at x = 0.
        let d = PieceKind::Bar.data(1);
        p.piece = Some(Active { kind: PieceKind::Bar, rot: 1, x: d.hot_x - d.left, y: 0 });
        p.waiting = false;
        p.hard_drop();
        assert_eq!(p.lines, 4);
        assert!(p.events.iter().any(|e| matches!(e, Event::SendLines(4))));
    }
}
