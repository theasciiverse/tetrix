//! Computer opponents, so a solo session still has five fields to attack.
//!
//! Placement uses the well-known four-feature heuristic (aggregate height,
//! completed lines, holes, bumpiness) over every rotation and column; the
//! bot then walks the piece there one key press at a time at a human-ish
//! pace, so it plays by the same rules and timings as you.

use crate::engine::field::{Field, WIDTH};
use crate::engine::piece::PieceKind;
use crate::engine::special::Special;
use crate::game::Slot;
use rand::{rngs::SmallRng, Rng, SeedableRng};

#[derive(Clone, Copy, Debug)]
pub struct Skill {
    /// Milliseconds between simulated key presses.
    pub move_ms: i64,
    /// Pause before acting on a fresh piece.
    pub think_ms: i64,
    /// Random noise added to each placement score; higher plays worse.
    pub noise: f64,
}

impl Skill {
    pub fn for_level(level: u8) -> Skill {
        match level {
            0 => Skill { move_ms: 260, think_ms: 450, noise: 2.5 },
            1 => Skill { move_ms: 170, think_ms: 300, noise: 1.0 },
            2 => Skill { move_ms: 110, think_ms: 180, noise: 0.35 },
            _ => Skill { move_ms: 60, think_ms: 90, noise: 0.0 },
        }
    }
}

pub enum BotSpecial {
    Use(usize),
    Discard,
}

#[derive(Clone, Copy)]
struct Plan {
    serial: u64,
    rot: usize,
    x: i32,
}

pub struct Bot {
    skill: Skill,
    plan: Option<Plan>,
    cooldown: i64,
    special_cooldown: i64,
    held: u32,
    rng: SmallRng,
}

impl Bot {
    pub fn new(skill: Skill, seed: u64) -> Self {
        Bot { skill, plan: None, cooldown: 0, special_cooldown: 3000, held: 0, rng: SmallRng::seed_from_u64(seed) }
    }

    pub fn update(&mut self, dt: i64, me: usize, slots: &mut [Slot]) -> Option<BotSpecial> {
        if !slots[me].player.alive {
            return None;
        }
        self.special_cooldown -= dt;
        let special = if self.special_cooldown <= 0 && !slots[me].player.inventory.is_empty() {
            self.special_cooldown = self.rng.gen_range(900..2600);
            self.choose_special(me, slots)
        } else {
            None
        };

        self.cooldown -= dt;
        if self.cooldown > 0 {
            return special;
        }
        let p = &mut slots[me].player;
        let Some(active) = p.piece else { return special };

        let plan = match self.plan {
            Some(plan) if plan.serial == p.piece_serial => plan,
            _ => {
                let (rot, x) = self.best_placement(&p.field, active.kind, active.y);
                let plan = Plan { serial: p.piece_serial, rot, x };
                self.plan = Some(plan);
                self.cooldown = self.skill.think_ms;
                return special;
            }
        };

        let moved = if active.rot != plan.rot {
            p.rotate(1)
        } else if active.x < plan.x {
            p.shift(1)
        } else if active.x > plan.x {
            p.shift(-1)
        } else {
            p.hard_drop();
            true
        };
        if !moved {
            // Blocked on the way (the field changed under us); just drop.
            p.hard_drop();
        }
        self.cooldown = self.skill.move_ms;
        special
    }

    fn best_placement(&mut self, field: &Field, kind: PieceKind, y0: i32) -> (usize, i32) {
        let mut best = (f64::MIN, 0, 6);
        for rot in 0..4 {
            for x in -3..WIDTH as i32 + 3 {
                if !field.fits(kind, rot, x, y0) {
                    continue;
                }
                let mut y = y0;
                while field.fits(kind, rot, x, y + 1) {
                    y += 1;
                }
                let mut f = field.clone();
                for (bx, by) in kind.data(rot).blocks(x, y) {
                    if by >= 0 {
                        f.set(bx as usize, by as usize, 1);
                    }
                }
                let lines = f.clear_full_rows().len() as f64;
                let noise = if self.skill.noise > 0.0 { self.rng.gen_range(0.0..self.skill.noise) } else { 0.0 };
                let score = evaluate(&f, lines) + noise;
                if score > best.0 {
                    best = (score, rot, x);
                }
            }
        }
        (best.1, best.2)
    }

    fn choose_special(&mut self, me: usize, slots: &[Slot]) -> Option<BotSpecial> {
        let sp = *slots[me].player.inventory.front()?;
        let my_h = slots[me].player.field.stack_height();
        let opponents: Vec<(usize, usize)> = slots
            .iter()
            .enumerate()
            .filter(|(j, s)| *j != me && s.player.alive)
            .map(|(j, s)| (j, s.player.field.stack_height()))
            .collect();
        let crowded = slots[me].player.inventory.len() > 4;

        let choice = match sp {
            Special::ClearLine | Special::Gravity if my_h >= 8 || crowded => Some(BotSpecial::Use(me)),
            Special::Nuke if my_h >= 13 => Some(BotSpecial::Use(me)),
            Special::Switch => opponents
                .iter()
                .min_by_key(|(_, h)| *h)
                .filter(|(_, h)| my_h >= h + 6)
                .map(|&(j, _)| BotSpecial::Use(j)),
            s if s.is_defensive() => None,
            _ if opponents.is_empty() => Some(BotSpecial::Discard),
            _ => {
                // Mostly hit the leader (lowest stack), sometimes anyone.
                let target = if self.rng.gen_bool(0.6) {
                    opponents.iter().min_by_key(|(_, h)| *h).map(|&(j, _)| j)
                } else {
                    Some(opponents[self.rng.gen_range(0..opponents.len())].0)
                };
                target.map(BotSpecial::Use)
            }
        };
        if choice.is_some() {
            self.held = 0;
            return choice;
        }
        self.held += 1;
        // Don't sit on a defensive block forever while attacks queue behind it.
        (self.held > 4).then(|| {
            self.held = 0;
            BotSpecial::Discard
        })
    }
}

/// Higher is better. Weights from Yiyuan Lee's well-known genetic-algorithm
/// tuning of this four-feature evaluation.
fn evaluate(f: &Field, lines: f64) -> f64 {
    let h = f.column_heights();
    let agg: usize = h.iter().sum();
    let bump: usize = h.windows(2).map(|w| w[0].abs_diff(w[1])).sum();
    let mut holes = 0;
    for (x, &hx) in h.iter().enumerate() {
        let top = crate::engine::field::HEIGHT - hx;
        holes += (top..crate::engine::field::HEIGHT).filter(|&y| f.get(x, y) == 0).count();
    }
    -0.510066 * agg as f64 + 0.760666 * lines - 0.35663 * holes as f64 - 0.184483 * bump as f64
}
