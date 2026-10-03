//! Juice: line-clear particle spray in Braille sub-cells, lock flashes,
//! attack shockwaves (board shake + border flash), banners.
//!
//! Particle coordinates are in terminal cells relative to the top-left of
//! your main field's interior, so the engine never needs to know where
//! anything is drawn.

use crate::engine::field::{Row, WIDTH};
use crate::engine::player::Event;
use crate::engine::special::Attack;
use crate::game::Notice;
use crate::ui::cell_color;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use std::collections::HashMap;

struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    color: Color,
}

pub struct Fx {
    particles: Vec<Particle>,
    /// Rows of your field flashing white just before they vanish.
    pub row_flash: Vec<(usize, f32)>,
    pub shake: f32,
    pub border_flash: Option<(Color, f32)>,
    pub screen_flash: f32,
    pub banner: Option<(String, Color, f32)>,
    /// Per-slot flash when an opponent's field gets hit.
    pub mini_flash: [f32; 6],
    me: usize,
    combo: u32,
    /// A lock this frame with no line clear after it ends the combo chain.
    lock_pending: bool,
    tick: u64,
    rng: SmallRng,
}

impl Fx {
    pub fn new(me: usize) -> Self {
        Fx {
            me,
            particles: Vec::new(),
            row_flash: Vec::new(),
            shake: 0.0,
            border_flash: None,
            screen_flash: 0.0,
            banner: None,
            mini_flash: [0.0; 6],
            combo: 0,
            lock_pending: false,
            tick: 0,
            rng: SmallRng::seed_from_u64(0x7e7),
        }
    }

    pub fn on_notice(&mut self, n: &Notice) {
        let me = self.me;
        match n {
            Notice::Player(i, Event::Locked { .. }) if *i == me => {
                self.lock_pending = true;
                if self.border_flash.is_none() {
                    self.border_flash = Some((Color::Rgb(200, 200, 200), 0.07));
                }
            }
            Notice::Player(i, Event::LinesCleared { rows, .. }) if *i == me => self.burst(rows),
            Notice::Player(i, Event::LevelUp(l)) if *i == me => {
                if self.banner.is_none() {
                    self.banner = Some((format!("LEVEL {l}"), Color::Rgb(255, 176, 0), 0.9));
                }
            }
            Notice::Player(i, Event::Lost) if *i == me => {
                self.shake = 0.5;
                self.border_flash = Some((Color::Red, 0.8));
            }
            Notice::Attack { from, to, attack } => {
                let hits_me = match to {
                    Some(t) => *t == me && *from != me,
                    None => *from != me,
                };
                let helpful = matches!(attack, Attack::Special(s) if s.is_defensive());
                if hits_me {
                    self.shake = self.shake.max(if helpful { 0.1 } else { 0.25 });
                    let c = if helpful { Color::Green } else { Color::Rgb(255, 60, 60) };
                    self.border_flash = Some((c, 0.3));
                } else if *to == Some(me) && helpful {
                    self.border_flash = Some((Color::Green, 0.3));
                }
                match to {
                    Some(t) if *t != me => self.mini_flash[*t] = 0.35,
                    None => {
                        for (i, f) in self.mini_flash.iter_mut().enumerate() {
                            if i != *from && i != me {
                                *f = 0.35;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Notice::Player(_, _) => {}
        }
    }

    /// A clear: rows flash, then explode outward as Braille sparks. More
    /// lines and longer combos make a bigger mess.
    fn burst(&mut self, rows: &[(usize, Row)]) {
        self.lock_pending = false;
        self.combo += 1;
        let n = rows.len();
        let per_cell = 2 + n + self.combo.min(6) as usize;
        for (y, row) in rows {
            self.row_flash.push((*y, 0.12));
            for (x, &c) in row.iter().enumerate() {
                for _ in 0..per_cell {
                    let side = if (x as f32) < WIDTH as f32 / 2.0 { -1.0 } else { 1.0 };
                    self.particles.push(Particle {
                        x: x as f32 * 2.0 + self.rng.gen_range(0.0..2.0),
                        y: *y as f32 + self.rng.gen_range(0.0..1.0),
                        vx: side * self.rng.gen_range(3.0..28.0) + self.rng.gen_range(-6.0..6.0),
                        vy: self.rng.gen_range(-14.0..3.0),
                        life: self.rng.gen_range(0.5..1.3),
                        color: cell_color(c),
                    });
                }
            }
        }
        if n >= 4 {
            self.banner = Some(("T E T R I X !".into(), Color::Rgb(255, 176, 0), 1.4));
            self.screen_flash = 0.25;
            self.shake = 0.35;
        } else if self.combo >= 3 {
            self.banner = Some((format!("COMBO x{}", self.combo), Color::Cyan, 0.8));
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.tick += 1;
        if self.lock_pending {
            self.lock_pending = false;
            self.combo = 0;
        }
        for p in &mut self.particles {
            p.vy += 30.0 * dt;
            p.vx *= 1.0 - 1.5 * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for f in &mut self.row_flash {
            f.1 -= dt;
        }
        self.row_flash.retain(|f| f.1 > 0.0);
        self.shake = (self.shake - dt).max(0.0);
        self.screen_flash = (self.screen_flash - dt).max(0.0);
        if let Some((_, t)) = &mut self.border_flash {
            *t -= dt;
            if *t <= 0.0 {
                self.border_flash = None;
            }
        }
        if let Some((_, _, t)) = &mut self.banner {
            *t -= dt;
            if *t <= 0.0 {
                self.banner = None;
            }
        }
        for f in &mut self.mini_flash {
            *f = (*f - dt).max(0.0);
        }
    }

    /// Horizontal offset for the shaking main board.
    pub fn shake_offset(&self) -> i32 {
        if self.shake > 0.0 {
            if self.tick % 2 == 0 { 1 } else { -1 }
        } else {
            0
        }
    }

    /// Draw particles as Braille dots, clipped to `clip`.
    /// `scale` is how many terminal rows one field row takes (cells are
    /// `2 * scale` columns wide), so the spray grows with the board.
    pub fn render_particles(&self, origin: (i32, i32), scale: f32, clip: Rect, buf: &mut Buffer) {
        let mut cells: HashMap<(i32, i32), (u8, Color)> = HashMap::new();
        for p in &self.particles {
            let dx = ((origin.0 as f32 + p.x * scale) * 2.0).floor() as i32;
            let dy = ((origin.1 as f32 + p.y * scale) * 4.0).floor() as i32;
            let (cx, cy) = (dx.div_euclid(2), dy.div_euclid(4));
            let bit = braille_bit(dx.rem_euclid(2), dy.rem_euclid(4));
            let e = cells.entry((cx, cy)).or_insert((0, p.color));
            e.0 |= bit;
            e.1 = p.color;
        }
        for ((cx, cy), (bits, color)) in cells {
            if cx < clip.left() as i32 || cy < clip.top() as i32 || cx >= clip.right() as i32 || cy >= clip.bottom() as i32 {
                continue;
            }
            let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
            if let Some(cell) = buf.cell_mut((cx as u16, cy as u16)) {
                cell.set_char(ch).set_fg(color);
            }
        }
    }
}

fn braille_bit(dx: i32, dy: i32) -> u8 {
    match (dx, dy) {
        (0, 0) => 0x01,
        (0, 1) => 0x02,
        (0, 2) => 0x04,
        (1, 0) => 0x08,
        (1, 1) => 0x10,
        (1, 2) => 0x20,
        (0, 3) => 0x40,
        _ => 0x80,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tetrix_clear_sprays_braille_and_banner() {
        let mut fx = Fx::new(0);
        let rows: Vec<(usize, Row)> = (18..22).map(|y| (y, [3u8; WIDTH])).collect();
        fx.on_notice(&Notice::Player(0, Event::LinesCleared { rows, specials_gained: 0 }));
        assert!(fx.banner.as_ref().is_some_and(|b| b.0.contains("T E T R I X")));
        fx.update(0.1);
        let area = Rect::new(0, 0, 80, 30);
        let mut buf = Buffer::empty(area);
        fx.render_particles((10, 2), 1.0, area, &mut buf);
        let braille = buf.content().iter().filter(|c| c.symbol().chars().any(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch))).count();
        assert!(braille > 20, "only {braille} braille cells");
    }
}
