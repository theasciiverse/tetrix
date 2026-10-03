//! A match of up to six players: routes specials and classic lines between
//! fields, averages levels, and decides who won.
//!
//! Each slot is either owned (simulated on this machine: you, or a bot on
//! the host) or a mirror of a player simulated elsewhere, updated from
//! `proto::Msg::State`. Everything an owned player does that others must
//! see goes to `outbox` as wire messages; `apply_remote` takes the other
//! direction. Offline, every slot is owned and the outbox is ignored.

use crate::bot::{Bot, BotSpecial};
use crate::engine::field::{HEIGHT, WIDTH};
use crate::engine::piece::PieceKind;
use crate::engine::player::{Active, Event, Player};
use crate::engine::settings::Settings;
use crate::engine::special::{self, Attack, Special};
use crate::proto::{Msg, PieceWire, StateWire};
use rand::{rngs::SmallRng, Rng, SeedableRng};
use std::collections::VecDeque;

pub const MAX_PLAYERS: usize = 6;
/// How often an owned player's falling piece is re-sent while it moves.
const PIECE_SYNC_MS: i64 = 100;

pub struct Slot {
    pub name: String,
    pub player: Player,
    pub bot: Option<Bot>,
    pub owned: bool,
    sent_piece: Option<Active>,
}

pub struct SlotSpec {
    pub name: String,
    pub bot: Option<Bot>,
    pub owned: bool,
}

/// Something the renderer should know about.
#[derive(Clone, Debug)]
pub enum Notice {
    Player(usize, Event),
    Attack { from: usize, to: Option<usize>, attack: Attack },
}

pub struct Outcome {
    pub winner: Option<usize>,
    /// Slots in the order they were eliminated.
    pub eliminated: Vec<usize>,
}

pub struct Match {
    pub slots: Vec<Slot>,
    pub log: VecDeque<String>,
    pub notices: Vec<Notice>,
    pub outcome: Option<Outcome>,
    pub elapsed_ms: i64,
    /// Wire messages for the other machines (drained by the app).
    pub outbox: Vec<Msg>,
    /// Decides when the round is over (offline, or the host).
    authority: bool,
    settings: Settings,
    eliminated: Vec<usize>,
    sync_timer: i64,
}

impl Match {
    pub fn new(settings: Settings, players: Vec<SlotSpec>, seed: u64, authority: bool) -> Self {
        let mut rng = SmallRng::seed_from_u64(seed);
        let piece_seed = rng.gen();
        let slots = players
            .into_iter()
            .take(MAX_PLAYERS)
            .map(|s| Slot {
                name: s.name,
                player: Player::new(settings.clone(), piece_seed, rng.gen()),
                bot: s.bot,
                owned: s.owned,
                sent_piece: None,
            })
            .collect();
        Match {
            slots,
            log: VecDeque::new(),
            notices: Vec::new(),
            outcome: None,
            elapsed_ms: 0,
            outbox: Vec::new(),
            authority,
            settings,
            eliminated: Vec::new(),
            sync_timer: 0,
        }
    }

    fn say(&mut self, msg: String) {
        log::info!("match: {msg}");
        self.log.push_back(msg);
        while self.log.len() > 50 {
            self.log.pop_front();
        }
    }

    pub fn alive(&self) -> impl Iterator<Item = usize> + '_ {
        self.slots.iter().enumerate().filter(|(_, s)| s.player.alive).map(|(i, _)| i)
    }

    pub fn tick(&mut self, dt_ms: i64) {
        if self.outcome.is_some() {
            return;
        }
        self.elapsed_ms += dt_ms;

        if self.settings.average_levels {
            let alive: Vec<u32> = self.slots.iter().filter(|s| s.player.alive).map(|s| s.player.level).collect();
            if !alive.is_empty() {
                let avg = alive.iter().sum::<u32>() / alive.len() as u32;
                for s in &mut self.slots {
                    s.player.speed_level = avg;
                }
            }
        }

        for i in 0..self.slots.len() {
            if !self.slots[i].owned {
                continue;
            }
            self.slots[i].player.tick(dt_ms);
            if let Some(mut bot) = self.slots[i].bot.take() {
                let action = bot.update(dt_ms, i, &mut self.slots);
                self.slots[i].bot = Some(bot);
                match action {
                    Some(BotSpecial::Use(to)) => self.use_special(i, to),
                    Some(BotSpecial::Discard) => {
                        self.slots[i].player.discard_special();
                    }
                    None => {}
                }
            }
        }
        self.route_events();

        self.sync_timer -= dt_ms;
        if self.sync_timer <= 0 {
            self.sync_timer = PIECE_SYNC_MS;
            for i in 0..self.slots.len() {
                let s = &self.slots[i];
                if s.owned && s.player.alive && s.player.piece != s.sent_piece {
                    self.send_state(i);
                }
            }
        }
    }

    fn send_state(&mut self, i: usize) {
        let s = &mut self.slots[i];
        s.sent_piece = s.player.piece;
        let p = &s.player;
        self.outbox.push(Msg::State(StateWire {
            slot: i as u8,
            field: p.field.rows().iter().flatten().copied().collect(),
            piece: p.piece.map(|a| PieceWire { kind: a.kind.index() as u8, rot: a.rot as u8, x: a.x as i8, y: a.y as i8 }),
            level: p.level as u16,
            lines: p.lines as u16,
            alive: p.alive,
        }));
    }

    /// Fire the front special of `from`'s inventory at `to`.
    pub fn use_special(&mut self, from: usize, to: usize) {
        if self.outcome.is_some() || to >= self.slots.len() || !self.slots[from].owned {
            return;
        }
        if !self.slots[from].player.alive || !self.slots[to].player.alive {
            return;
        }
        let Some(sp) = self.slots[from].player.inventory.pop_front() else { return };
        self.outbox.push(Msg::Special { from: from as u8, to: to as u8, special: sp.index() as u8 });
        self.special_landed(from, to, sp);
        self.route_events();
    }

    /// Log a special and apply whatever part of it lands on players this
    /// machine owns.
    fn special_landed(&mut self, from: usize, to: usize, sp: Special) {
        let (fname, tname) = (self.slots[from].name.clone(), self.slots[to].name.clone());
        if from == to {
            self.say(format!("{fname} used {} on self", sp.name()));
        } else {
            self.say(format!("{fname} used {} on {tname}", sp.name()));
        }
        self.notices.push(Notice::Attack { from, to: Some(to), attack: Attack::Special(sp) });

        if sp == Special::Switch {
            if from == to {
                return;
            }
            // Each owner takes the other's field as last seen here; that is
            // exactly how TetriNET clients resolved a switch.
            let (from_field, to_field) = (self.slots[from].player.field.clone(), self.slots[to].player.field.clone());
            for (i, field) in [(from, to_field), (to, from_field)] {
                let s = &mut self.slots[i];
                if s.owned && s.player.alive {
                    s.player.field = field;
                    special::lower_after_switch(&mut s.player.field);
                    s.player.after_field_change();
                }
            }
        } else if self.slots[to].owned {
            self.slots[to].player.receive(Attack::Special(sp));
        }
    }

    fn lines_landed(&mut self, from: usize, n: u8) {
        let name = self.slots[from].name.clone();
        self.say(format!("{name} adds {n} line{} to all", if n == 1 { "" } else { "s" }));
        self.notices.push(Notice::Attack { from, to: None, attack: Attack::ClassicLines(n) });
        for (j, s) in self.slots.iter_mut().enumerate() {
            if j != from && s.owned {
                s.player.receive(Attack::ClassicLines(n));
            }
        }
    }

    fn player_out(&mut self, i: usize) {
        if self.eliminated.contains(&i) {
            return;
        }
        let name = self.slots[i].name.clone();
        self.say(format!("{name} is out"));
        self.eliminated.push(i);
    }

    fn route_events(&mut self) {
        loop {
            let mut pending = Vec::new();
            for (i, s) in self.slots.iter_mut().enumerate() {
                pending.extend(s.player.events.drain(..).map(|e| (i, e)));
            }
            if pending.is_empty() {
                break;
            }
            for (i, e) in pending {
                match &e {
                    Event::SendLines(n) => {
                        self.outbox.push(Msg::Lines { from: i as u8, n: *n });
                        self.lines_landed(i, *n);
                    }
                    Event::FieldChanged => self.send_state(i),
                    Event::Lost => {
                        self.send_state(i);
                        self.outbox.push(Msg::Lost { slot: i as u8 });
                        self.player_out(i);
                    }
                    _ => {}
                }
                self.notices.push(Notice::Player(i, e));
            }
        }
        self.check_over();
    }

    /// Apply a gameplay message that came from another machine.
    pub fn apply_remote(&mut self, msg: &Msg) {
        let n = self.slots.len();
        match msg {
            Msg::State(s) => {
                let i = s.slot as usize;
                if i >= n || self.slots[i].owned || s.field.len() != WIDTH * HEIGHT {
                    return;
                }
                let p = &mut self.slots[i].player;
                for (k, &c) in s.field.iter().enumerate() {
                    p.field.set(k % WIDTH, k / WIDTH, c);
                }
                p.piece = s.piece.as_ref().and_then(|w| {
                    PieceKind::ALL.get(w.kind as usize).map(|&kind| Active {
                        kind,
                        rot: w.rot as usize % 4,
                        x: w.x as i32,
                        y: w.y as i32,
                    })
                });
                p.level = s.level as u32;
                p.lines = s.lines as u32;
                p.alive = s.alive;
            }
            Msg::Special { from, to, special } => {
                let (f, t) = (*from as usize, *to as usize);
                let Some(&sp) = Special::ALL.get(*special as usize) else { return };
                if f < n && t < n && !self.slots[f].owned && self.outcome.is_none() {
                    self.special_landed(f, t, sp);
                }
            }
            Msg::Lines { from, n: lines } => {
                let f = *from as usize;
                if f < n && !self.slots[f].owned && self.outcome.is_none() {
                    self.lines_landed(f, *lines);
                }
            }
            Msg::Lost { slot } => {
                let i = *slot as usize;
                if i < n && !self.slots[i].owned {
                    self.slots[i].player.alive = false;
                    self.slots[i].player.piece = None;
                    self.player_out(i);
                }
            }
            Msg::Over { winner, eliminated } => {
                if !self.authority {
                    let winner = winner.map(|w| w as usize).filter(|&w| w < n);
                    if let Some(w) = winner {
                        let name = self.slots[w].name.clone();
                        self.say(format!("{name} wins!"));
                    }
                    self.outcome =
                        Some(Outcome { winner, eliminated: eliminated.iter().map(|&e| e as usize).collect() });
                }
            }
            _ => {}
        }
        self.route_events();
    }

    /// A remote player disconnected mid-round (host only): they're out.
    pub fn drop_player(&mut self, i: usize) {
        if i < self.slots.len() && self.slots[i].player.alive {
            self.slots[i].player.alive = false;
            self.slots[i].player.piece = None;
            self.outbox.push(Msg::Lost { slot: i as u8 });
            self.player_out(i);
            self.check_over();
        }
    }

    fn check_over(&mut self) {
        if self.outcome.is_some() || !self.authority {
            return;
        }
        let alive: Vec<usize> = self.alive().collect();
        let multi = self.slots.len() > 1;
        if (multi && alive.len() <= 1) || alive.is_empty() {
            let winner = if multi { alive.first().copied() } else { None };
            if let Some(w) = winner {
                let name = self.slots[w].name.clone();
                self.say(format!("{name} wins!"));
            }
            self.outbox.push(Msg::Over {
                winner: winner.map(|w| w as u8),
                eliminated: self.eliminated.iter().map(|&e| e as u8).collect(),
            });
            self.outcome = Some(Outcome { winner, eliminated: self.eliminated.clone() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::Skill;

    fn bots(n: usize, seed: u64, owned: impl Fn(usize) -> bool) -> Vec<SlotSpec> {
        (0..n)
            .map(|i| SlotSpec {
                name: format!("b{i}"),
                bot: owned(i).then(|| Bot::new(Skill::for_level((i % 4) as u8), seed * 10 + i as u64)),
                owned: owned(i),
            })
            .collect()
    }

    fn chaotic() -> Settings {
        let mut s = Settings::default();
        // Every special equally likely, so switch/nuke/gravity get exercised.
        s.special_freq = [12, 11, 11, 11, 11, 11, 11, 11, 11];
        s
    }

    /// Six bots play whole matches headlessly: exercises every special,
    /// switch, classic lines and elimination, and checks a winner emerges.
    #[test]
    fn bot_matches_finish() {
        for seed in 0..6u64 {
            let mut m = Match::new(chaotic(), bots(6, seed, |_| true), seed, true);
            let mut used = std::collections::HashSet::new();
            let mut t = 0;
            while m.outcome.is_none() && t < 30 * 60 * 1000 {
                m.tick(16);
                for n in m.notices.drain(..) {
                    if let Notice::Attack { attack: Attack::Special(s), .. } = n {
                        used.insert(s);
                    }
                }
                m.outbox.clear();
                t += 16;
            }
            let o = m.outcome.as_ref().unwrap_or_else(|| panic!("seed {seed}: no winner after 30 simulated minutes"));
            assert!(o.winner.is_some());
            assert_eq!(o.eliminated.len(), 5);
            assert!(used.len() >= 5, "seed {seed}: only {used:?} were used");
        }
    }

    /// The same match split across three "machines" (a host owning slots
    /// 0-1 and two joiners owning 2-3 and 4-5), wired together through a
    /// host relay exactly like the real network: every machine must agree
    /// on every field and on the winner.
    #[test]
    fn split_match_stays_in_sync() {
        for seed in 0..4u64 {
            let owners = [0usize, 0, 1, 1, 2, 2];
            let mut machines: Vec<Match> = (0..3)
                .map(|m| Match::new(chaotic(), bots(6, seed, |i| owners[i] == m), seed, m == 0))
                .collect();
            let mut t = 0;
            while machines.iter().any(|m| m.outcome.is_none()) && t < 30 * 60 * 1000 {
                for m in machines.iter_mut() {
                    m.tick(16);
                    m.notices.clear();
                }
                // Joiners -> host -> everyone else; host -> everyone.
                for _ in 0..3 {
                    for src in 0..3 {
                        let out: Vec<Msg> = machines[src].outbox.drain(..).collect();
                        for msg in out {
                            for dst in 0..3 {
                                if dst != src {
                                    machines[dst].apply_remote(&msg);
                                }
                            }
                        }
                    }
                }
                t += 16;
            }
            let winners: Vec<Option<usize>> =
                machines.iter().map(|m| m.outcome.as_ref().expect("every machine sees the end").winner).collect();
            assert!(winners[0].is_some(), "seed {seed}");
            assert!(winners.iter().all(|w| *w == winners[0]), "seed {seed}: {winners:?}");
            for i in 0..6 {
                let f = &machines[owners[i]].slots[i].player.field;
                for m in &machines {
                    assert_eq!(&m.slots[i].player.field, f, "seed {seed}: slot {i} field diverged");
                }
            }
        }
    }
}
