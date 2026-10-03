//! Tetrix: TetriNET-style multiplayer Tetris for the terminal.
//!
//! Screens: a title menu (solo, host, join), a LAN browser that lists games
//! found on the network, a lobby, and the game itself. Solo plays against
//! local bots; hosting makes this machine the relay for up to five joiners
//! (plus bots), discovered over the LAN beacon and connected with iroh.

mod bot;
mod engine;
mod fx;
mod game;
mod lan;
mod net;
mod proto;
mod ui;

use bot::{Bot, Skill};
use engine::settings::Settings;
use fx::Fx;
use game::{Match, Notice, SlotSpec, MAX_PLAYERS};
use net::{ClientId, Cmd, Evt, Net};
use proto::{LobbyEntry, Msg};
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const BOT_NAMES: [&str; 5] = ["Ada", "Blitz", "Cobol", "Dvorak", "Emacs"];
pub const SKILL_NAMES: [&str; 4] = ["easy", "normal", "hard", "brutal"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Menu,
    Browse,
    Lobby,
    Game,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Solo,
    Host,
    Join,
    Name,
    Bots,
    Skill,
    Quit,
}

pub const MENU: [MenuItem; 7] =
    [MenuItem::Solo, MenuItem::Host, MenuItem::Join, MenuItem::Name, MenuItem::Bots, MenuItem::Skill, MenuItem::Quit];

pub struct Game {
    pub mat: Match,
    pub fx: Fx,
    /// Which slot is you on this machine.
    pub me: usize,
    pub paused: bool,
    pub round: u32,
    /// Joiners can't skip ahead or pause; only the host or a solo player can.
    pub networked: bool,
    pub is_host: bool,
}

pub struct Session {
    pub net: Net,
    pub host: bool,
    pub status: String,
    pub endpoint: Option<String>,
    pub host_name: String,
    /// What the lobby shows: host first, then joiners, then bots.
    pub players: Vec<LobbyEntry>,
    /// Host: joiners in arrival order.
    pub clients: Vec<(ClientId, String)>,
    /// Host: which slot each joiner plays this round.
    pub slot_of: HashMap<ClientId, usize>,
    pub connected: bool,
    ping: f32,
}

pub struct App {
    pub screen: Screen,
    pub menu_sel: usize,
    pub editing_name: bool,
    pub name: String,
    pub bots: usize,
    pub skill: u8,
    pub shadow: bool,
    pub help: bool,
    pub browse: Option<lan::Listener>,
    pub browse_sel: usize,
    pub hosts: Vec<lan::Found>,
    pub session: Option<Session>,
    pub game: Option<Game>,
    /// TetriNET scoring: 3 points to the winner, 2 to the last player out,
    /// 1 to the one before.
    pub winlist: Vec<(String, u32)>,
    pub toast: Option<(String, f32)>,
    settings: Settings,
    seed: Option<u64>,
    round: u32,
    scored: bool,
    quit: bool,
}

fn fresh_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

impl App {
    fn toast(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        log::info!("toast: {msg}");
        self.toast = Some((msg, 4.0));
    }

    fn to_menu(&mut self) {
        self.session = None;
        self.game = None;
        self.browse = None;
        self.screen = Screen::Menu;
        log::info!("screen -> menu");
    }

    // ---- solo -------------------------------------------------------------

    fn start_solo(&mut self) {
        self.round += 1;
        let seed = self.seed.unwrap_or_else(fresh_seed) ^ (self.round as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut players = vec![SlotSpec { name: self.name.clone(), bot: None, owned: true }];
        for (i, n) in BOT_NAMES.iter().take(self.bots).enumerate() {
            players.push(SlotSpec {
                name: n.to_string(),
                bot: Some(Bot::new(Skill::for_level(self.skill), seed ^ (i as u64 + 1))),
                owned: true,
            });
        }
        log::info!("solo round {} seed={seed} players={}", self.round, players.len());
        self.begin_game(Match::new(self.settings.clone(), players, seed, true), 0, false, false);
    }

    fn begin_game(&mut self, mut mat: Match, me: usize, networked: bool, is_host: bool) {
        mat.log.push_back("Get ready...".into());
        self.game = Some(Game { mat, fx: Fx::new(me), me, paused: false, round: self.round, networked, is_host });
        self.scored = false;
        self.screen = Screen::Game;
    }

    // ---- hosting ----------------------------------------------------------

    fn start_hosting(&mut self) {
        let host_name = format!("{}'s game", self.name);
        self.session = Some(Session {
            net: Net::host(host_name.clone()),
            host: true,
            status: "Opening the network...".into(),
            endpoint: None,
            host_name,
            players: Vec::new(),
            clients: Vec::new(),
            slot_of: HashMap::new(),
            connected: true,
            ping: 0.0,
        });
        self.round = 0;
        self.screen = Screen::Lobby;
        self.refresh_lobby();
        log::info!("screen -> lobby (host)");
    }

    /// Host: how many bots fit next to the humans.
    fn lobby_bots(&self) -> usize {
        let humans = 1 + self.session.as_ref().map_or(0, |s| s.clients.len());
        self.bots.min(MAX_PLAYERS - humans.min(MAX_PLAYERS))
    }

    /// Host: rebuild the lobby list and tell everyone.
    fn refresh_lobby(&mut self) {
        let bots = self.lobby_bots();
        let name = self.name.clone();
        let in_game = self.game.as_ref().is_some_and(|g| g.mat.outcome.is_none());
        let Some(s) = self.session.as_mut() else { return };
        if !s.host {
            return;
        }
        let mut players = vec![LobbyEntry { name, bot: false }];
        players.extend(s.clients.iter().map(|(_, n)| LobbyEntry { name: n.clone(), bot: false }));
        players.extend(BOT_NAMES.iter().take(bots).map(|n| LobbyEntry { name: n.to_string(), bot: true }));
        s.players = players.clone();
        s.net.send(Cmd::Broadcast(Msg::Lobby { players, in_game }));
        s.net.send(Cmd::Beacon { players: (1 + s.clients.len()) as u8, in_game });
    }

    fn host_start_round(&mut self) {
        let bots = self.lobby_bots();
        let Some(s) = self.session.as_mut() else { return };
        let total = 1 + s.clients.len() + bots;
        if total < 2 {
            self.toast("Need at least one other player or a bot (Right arrow adds bots)");
            return;
        }
        self.round += 1;
        let seed = fresh_seed();
        let entries = s.players.clone();
        let mut specs = vec![SlotSpec { name: self.name.clone(), bot: None, owned: true }];
        s.slot_of.clear();
        for (k, (client, name)) in s.clients.iter().enumerate() {
            s.slot_of.insert(*client, k + 1);
            specs.push(SlotSpec { name: name.clone(), bot: None, owned: false });
        }
        for (i, n) in BOT_NAMES.iter().take(bots).enumerate() {
            let bot = Bot::new(Skill::for_level(self.skill), seed ^ (i as u64 + 1));
            specs.push(SlotSpec { name: n.to_string(), bot: Some(bot), owned: true });
        }
        for (client, slot) in &s.slot_of {
            s.net.send(Cmd::SendTo(
                *client,
                Msg::Start {
                    round: self.round,
                    seed,
                    settings: self.settings.clone(),
                    players: entries.clone(),
                    you: *slot as u8,
                },
            ));
        }
        log::info!("host round {} seed={seed} players={total}", self.round);
        self.begin_game(Match::new(self.settings.clone(), specs, seed, true), 0, true, true);
        self.refresh_lobby();
    }

    fn host_event(&mut self, e: Evt) {
        match e {
            Evt::Hosting { id } => {
                if let Some(s) = self.session.as_mut() {
                    s.endpoint = Some(id.to_string());
                    s.status = "Visible on the local network".into();
                }
            }
            Evt::Failed(why) => {
                if let Some(s) = self.session.as_mut() {
                    s.status = format!("Network problem: {why}");
                }
            }
            Evt::ClientJoined { client, name } => {
                let s = self.session.as_mut().unwrap();
                let name: String = name.chars().filter(|c| !c.is_control()).take(10).collect();
                if 1 + s.clients.len() >= MAX_PLAYERS {
                    s.net.send(Cmd::SendTo(client, Msg::Reject { reason: "the game is full (6 players)".into() }));
                    s.net.send(Cmd::Kick(client));
                    return;
                }
                s.clients.push((client, name.clone()));
                let waiting = self.game.as_ref().is_some_and(|g| g.mat.outcome.is_none());
                self.toast(if waiting { format!("{name} joined, plays next round") } else { format!("{name} joined") });
                self.refresh_lobby();
            }
            Evt::ClientLeft { client } => {
                let s = self.session.as_mut().unwrap();
                let name = s.clients.iter().find(|(c, _)| *c == client).map(|(_, n)| n.clone());
                s.clients.retain(|(c, _)| *c != client);
                if let (Some(slot), Some(g)) = (s.slot_of.remove(&client), self.game.as_mut()) {
                    g.mat.drop_player(slot);
                }
                if let Some(n) = name {
                    self.toast(format!("{n} left"));
                }
                self.refresh_lobby();
            }
            Evt::ClientMsg { client, msg } => self.host_relay(client, msg),
            _ => {}
        }
    }

    /// Host: apply a joiner's gameplay message here and pass it on to the
    /// other joiners, refusing anything about a slot they don't own.
    fn host_relay(&mut self, client: ClientId, msg: Msg) {
        let Some(s) = self.session.as_ref() else { return };
        let Some(&slot) = s.slot_of.get(&client) else { return };
        let claims = match &msg {
            Msg::State(st) => Some(st.slot),
            Msg::Special { from, .. } | Msg::Lines { from, .. } => Some(*from),
            Msg::Lost { slot } => Some(*slot),
            _ => None,
        };
        if claims != Some(slot as u8) {
            return;
        }
        if let Some(g) = self.game.as_mut() {
            g.mat.apply_remote(&msg);
        }
        s.net.send(Cmd::SendExcept(client, msg));
    }

    // ---- joining ----------------------------------------------------------

    fn start_browse(&mut self) {
        match lan::Listener::start(lan::PORT) {
            Ok(l) => {
                self.browse = Some(l);
                self.browse_sel = 0;
                self.hosts.clear();
                self.screen = Screen::Browse;
                log::info!("screen -> browse");
            }
            Err(e) => self.toast(format!("Can't listen for LAN games: {e}")),
        }
    }

    fn join(&mut self, addr: iroh::EndpointAddr, host_name: String) {
        log::info!("joining {host_name} at {addr:?}");
        self.browse = None;
        self.session = Some(Session {
            net: Net::join(addr, self.name.clone()),
            host: false,
            status: format!("Connecting to {host_name}..."),
            endpoint: None,
            host_name,
            players: Vec::new(),
            clients: Vec::new(),
            slot_of: HashMap::new(),
            connected: false,
            ping: 0.0,
        });
        self.screen = Screen::Lobby;
    }

    fn join_event(&mut self, e: Evt) {
        match e {
            Evt::Connected => {
                let s = self.session.as_mut().unwrap();
                s.connected = true;
                s.status = "Connected. Waiting for the host to start.".into();
            }
            Evt::Failed(why) => {
                self.to_menu();
                self.toast(format!("Couldn't join: {why}"));
            }
            Evt::Disconnected => {
                self.to_menu();
                self.toast("The host closed the game");
            }
            Evt::HostMsg(msg) => match msg {
                Msg::Reject { reason } => {
                    self.to_menu();
                    self.toast(format!("Host said no: {reason}"));
                }
                Msg::Lobby { players, in_game } => {
                    let s = self.session.as_mut().unwrap();
                    s.players = players;
                    s.status = if in_game {
                        "A round is running. You're in the next one.".into()
                    } else {
                        "Connected. Waiting for the host to start.".into()
                    };
                }
                Msg::ToLobby => {
                    if self.game.as_ref().is_some_and(|g| g.mat.outcome.is_some()) {
                        self.game = None;
                        self.screen = Screen::Lobby;
                    }
                }
                Msg::Start { round, seed, settings, players, you } => {
                    let me = you as usize;
                    let specs = players
                        .iter()
                        .enumerate()
                        .map(|(i, p)| SlotSpec { name: p.name.clone(), bot: None, owned: i == me })
                        .collect();
                    self.round = round;
                    log::info!("joined round {round} seed={seed} as slot {me}");
                    self.begin_game(Match::new(settings, specs, seed, false), me, true, false);
                }
                other => {
                    if let Some(g) = self.game.as_mut() {
                        g.mat.apply_remote(&other);
                    }
                }
            },
            _ => {}
        }
    }

    // ---- input ------------------------------------------------------------

    fn on_key(&mut self, k: KeyEvent) {
        log::debug!("key {:?} mods {:?} kind {:?} screen {:?}", k.code, k.modifiers, k.kind, self.screen);
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        if self.help {
            if matches!(k.code, KeyCode::Char('?') | KeyCode::Esc | KeyCode::F(1) | KeyCode::Enter | KeyCode::Char('q')) {
                self.help = false;
            }
            return;
        }
        match self.screen {
            Screen::Menu => self.menu_key(k),
            Screen::Browse => self.browse_key(k),
            Screen::Lobby => self.lobby_key(k),
            Screen::Game => self.game_key(k),
        }
    }

    fn menu_key(&mut self, k: KeyEvent) {
        if self.editing_name {
            match k.code {
                KeyCode::Enter | KeyCode::Esc => {
                    if self.name.trim().is_empty() {
                        self.name = "player".into();
                    }
                    self.editing_name = false;
                }
                KeyCode::Backspace => {
                    self.name.pop();
                }
                KeyCode::Char(c) if !c.is_control() && c != '|' && self.name.chars().count() < 10 => self.name.push(c),
                _ => {}
            }
            return;
        }
        let item = MENU[self.menu_sel];
        let delta = match k.code {
            KeyCode::Left => -1,
            KeyCode::Right => 1,
            _ => 0,
        };
        match k.code {
            KeyCode::Up | KeyCode::Char('k') => self.menu_sel = (self.menu_sel + MENU.len() - 1) % MENU.len(),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => self.menu_sel = (self.menu_sel + 1) % MENU.len(),
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('?') => self.help = true,
            KeyCode::Enter | KeyCode::Char(' ') => match item {
                MenuItem::Solo => {
                    self.round = 0;
                    self.start_solo()
                }
                MenuItem::Host => self.start_hosting(),
                MenuItem::Join => self.start_browse(),
                MenuItem::Name => self.editing_name = true,
                MenuItem::Bots => self.bots = (self.bots + 1) % 6,
                MenuItem::Skill => self.skill = (self.skill + 1) % 4,
                MenuItem::Quit => self.quit = true,
            },
            _ if delta != 0 => match item {
                MenuItem::Bots => self.bots = (self.bots as i32 + delta).clamp(0, 5) as usize,
                MenuItem::Skill => self.skill = (self.skill as i32 + delta).clamp(0, 3) as u8,
                _ => {}
            },
            _ => {}
        }
    }

    fn browse_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => self.to_menu(),
            KeyCode::Up | KeyCode::Char('k') => self.browse_sel = self.browse_sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.browse_sel = (self.browse_sel + 1).min(self.hosts.len().saturating_sub(1))
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(f) = self.hosts.get(self.browse_sel).cloned() {
                    self.join(f.addr(), f.beacon.name.clone());
                }
            }
            _ => {}
        }
    }

    fn lobby_key(&mut self, k: KeyEvent) {
        let host = self.session.as_ref().is_some_and(|s| s.host);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => self.to_menu(),
            KeyCode::Enter | KeyCode::Char(' ') if host => self.host_start_round(),
            KeyCode::Left | KeyCode::Char('-') if host => {
                self.bots = self.lobby_bots().saturating_sub(1);
                self.refresh_lobby();
            }
            KeyCode::Right | KeyCode::Char('+') if host => {
                self.bots = (self.lobby_bots() + 1).min(5);
                self.refresh_lobby();
            }
            KeyCode::Char('s') if host => self.skill = (self.skill + 1) % 4,
            _ => {}
        }
    }

    fn game_key(&mut self, k: KeyEvent) {
        let Some(g) = self.game.as_mut() else { return };
        if matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) {
            // In a network game, q after you're out (or after the round)
            // goes back to the lobby; q mid-round leaves the session.
            if g.networked && (g.mat.outcome.is_some() || !g.mat.slots[g.me].player.alive) {
                self.game = None;
                self.screen = Screen::Lobby;
            } else {
                self.to_menu();
            }
            return;
        }
        if matches!(k.code, KeyCode::Char('?') | KeyCode::F(1)) {
            self.help = true;
            return;
        }
        if g.mat.outcome.is_some() {
            match k.code {
                KeyCode::Enter | KeyCode::Char('r') if !g.networked => self.start_solo(),
                KeyCode::Enter | KeyCode::Char('r') if g.is_host => self.host_start_round(),
                KeyCode::Char('l') if g.is_host => {
                    self.game = None;
                    self.screen = Screen::Lobby;
                    if let Some(s) = &self.session {
                        s.net.send(Cmd::Broadcast(Msg::ToLobby));
                    }
                    self.refresh_lobby();
                }
                _ => {}
            }
            return;
        }
        let me_idx = g.me;
        if !g.mat.slots[me_idx].player.alive {
            // Knocked out while bots play on: Enter forfeits the rest of a solo round.
            if k.code == KeyCode::Enter && !g.networked {
                self.start_solo();
            }
            return;
        }
        if k.code == KeyCode::Char('p') && !g.networked {
            g.paused = !g.paused;
            log::info!("paused={}", g.paused);
            return;
        }
        if g.paused {
            return;
        }
        let me = &mut g.mat.slots[me_idx].player;
        match k.code {
            KeyCode::Left | KeyCode::Char('h') => {
                me.shift(-1);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                me.shift(1);
            }
            KeyCode::Up | KeyCode::Char('x') | KeyCode::Char('k') => {
                me.rotate(1);
            }
            KeyCode::Char('z') => {
                me.rotate(-1);
            }
            KeyCode::Down | KeyCode::Char('j') => me.step_down(),
            KeyCode::Char(' ') => me.hard_drop(),
            KeyCode::Char('d') => {
                if let Some(s) = me.discard_special() {
                    g.mat.log.push_back(format!("you discarded {}", s.name()));
                }
            }
            KeyCode::Char('s') => self.shadow = !self.shadow,
            KeyCode::Char(c @ '1'..='6') => {
                let to = c as usize - '1' as usize;
                g.mat.use_special(me_idx, to);
            }
            _ => {}
        }
    }

    // ---- per frame --------------------------------------------------------

    fn update(&mut self, dt: Duration) {
        let secs = dt.as_secs_f32();
        if let Some((_, t)) = &mut self.toast {
            *t -= secs;
            if *t <= 0.0 {
                self.toast = None;
            }
        }
        if let Some(l) = &self.browse {
            self.hosts = l.hosts();
            self.browse_sel = self.browse_sel.min(self.hosts.len().saturating_sub(1));
        }

        // Network events first, so remote attacks land before we tick.
        let mut events = Vec::new();
        if let Some(s) = &self.session {
            while let Some(e) = s.net.poll() {
                events.push(e);
            }
        }
        for e in events {
            if !matches!(e, Evt::ClientMsg { msg: Msg::State(_) | Msg::Ping, .. } | Evt::HostMsg(Msg::State(_) | Msg::Ping)) {
                log::debug!("net event {e:?}");
            }
            if self.session.as_ref().is_some_and(|s| s.host) {
                self.host_event(e);
            } else if self.session.is_some() {
                self.join_event(e);
            }
        }

        if let Some(g) = self.game.as_mut() {
            if g.networked || (!g.paused && !self.help) {
                g.mat.tick(dt.as_millis() as i64);
            }
            for n in std::mem::take(&mut g.mat.notices) {
                if let Notice::Player(i, e) = &n {
                    if !matches!(e, engine::player::Event::FieldChanged | engine::player::Event::Locked { .. }) {
                        log::debug!("event slot {i}: {e:?}");
                    }
                }
                g.fx.on_notice(&n);
            }
            g.fx.update(secs);
            let out: Vec<Msg> = g.mat.outbox.drain(..).collect();
            if let Some(s) = &self.session {
                for m in out {
                    s.net.send(Cmd::Broadcast(m));
                }
            }
            if g.mat.outcome.is_some() && !self.scored {
                self.score_round();
                if self.session.as_ref().is_some_and(|s| s.host) {
                    self.refresh_lobby();
                }
            }
        }

        if let Some(s) = self.session.as_mut() {
            s.ping -= secs;
            if s.ping <= 0.0 {
                s.ping = 2.0;
                s.net.send(Cmd::Broadcast(Msg::Ping));
            }
        }
    }

    fn score_round(&mut self) {
        self.scored = true;
        let Some(g) = &self.game else { return };
        let Some(o) = g.mat.outcome.as_ref() else { return };
        if g.mat.slots.len() < 2 {
            return;
        }
        let mut awards: Vec<(usize, u32)> = Vec::new();
        if let Some(w) = o.winner {
            awards.push((w, 3));
        }
        for (pts, &slot) in [2u32, 1].iter().zip(o.eliminated.iter().rev()) {
            awards.push((slot, *pts));
        }
        for (slot, pts) in awards {
            let name = g.mat.slots[slot].name.clone();
            match self.winlist.iter_mut().find(|(n, _)| *n == name) {
                Some(e) => e.1 += pts,
                None => self.winlist.push((name, pts)),
            }
        }
        self.winlist.sort_by(|a, b| b.1.cmp(&a.1));
        log::info!("round {} over, winlist {:?}", g.round, self.winlist);
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: tetrix [options]

With no options Tetrix opens its menu: play solo, host a LAN game, or join one.

  --solo          skip the menu, start a solo game against bots
  --host          skip the menu, host a game (others find it automatically)
  --join          skip the menu, list games on the local network
  --join ID       join a host directly by its iroh id (works across networks)
  --bots N        computer opponents, 0-5 (default 3)
  --skill N       bot skill 0-3 (default 1)
  --name NAME     your name (default $USER)
  --level N       starting level 1-100 (default 1)
  --classic-delay 600 ms pause before each new piece, like classic TetriNET
  --slide         let hard-dropped pieces slide before locking
  --no-classic    disable classic-rules line sending on multi-line clears
  --noshadow      hide the landing shadow
  --seed N        fixed random seed (solo)

Logging: RUST_LOG=debug tetrix 2>tetrix.log"
    );
    std::process::exit(2)
}

enum Launch {
    Menu,
    Solo,
    Host,
    Browse,
    JoinId(iroh::EndpointId),
}

fn parse_args() -> (App, Launch) {
    let mut app = App {
        screen: Screen::Menu,
        menu_sel: 0,
        editing_name: false,
        name: std::env::var("USER").unwrap_or_else(|_| "player".into()),
        bots: 3,
        skill: 1,
        shadow: true,
        help: false,
        browse: None,
        browse_sel: 0,
        hosts: Vec::new(),
        session: None,
        game: None,
        winlist: Vec::new(),
        toast: None,
        settings: Settings::default(),
        seed: None,
        round: 0,
        scored: false,
        quit: false,
    };
    let mut launch = Launch::Menu;
    let mut args = std::env::args().skip(1).peekable();
    while let Some(a) = args.next() {
        let mut num = |name: &str| -> u64 {
            args.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                eprintln!("{name} needs a number");
                usage()
            })
        };
        match a.as_str() {
            "--bots" => app.bots = num("--bots").min(5) as usize,
            "--skill" => app.skill = num("--skill").min(3) as u8,
            "--level" => app.settings.initial_level = num("--level").clamp(1, 100) as u32,
            "--seed" => app.seed = Some(num("--seed")),
            "--name" => app.name = args.next().unwrap_or_else(|| usage()),
            "--classic-delay" => app.settings.spawn_delay_ms = 600,
            "--slide" => app.settings.slide = true,
            "--no-classic" => app.settings.classic = false,
            "--noshadow" => app.shadow = false,
            "--solo" => launch = Launch::Solo,
            "--host" => launch = Launch::Host,
            "--join" => {
                launch = match args.peek().filter(|v| !v.starts_with("--")).cloned() {
                    Some(id) => {
                        let id = id.parse().unwrap_or_else(|_| {
                            eprintln!("--join: {id:?} is not an iroh endpoint id");
                            usage()
                        });
                        args.next();
                        Launch::JoinId(id)
                    }
                    None => Launch::Browse,
                }
            }
            _ => usage(),
        }
    }
    app.name = app.name.chars().filter(|c| !c.is_control() && *c != '|').take(10).collect();
    (app, launch)
}

fn main() -> std::io::Result<()> {
    // Session logging per the Asciiverse convention: off unless RUST_LOG is
    // set; stderr is free because the TUI only owns stdout.
    // iroh's QUIC internals are very chatty at debug; keep them at warn
    // unless RUST_LOG names them explicitly.
    env_logger::Builder::new()
        .filter_module("noq", log::LevelFilter::Warn)
        .filter_module("noq_proto", log::LevelFilter::Warn)
        .filter_module("iroh", log::LevelFilter::Warn)
        .parse_default_env()
        .format_timestamp_millis()
        .init();
    log::info!("tetrix starting, args={:?}", std::env::args().collect::<Vec<_>>());

    let (mut app, launch) = parse_args();
    match launch {
        Launch::Menu => {}
        Launch::Solo => app.start_solo(),
        Launch::Host => app.start_hosting(),
        Launch::Browse => app.start_browse(),
        Launch::JoinId(id) => app.join(iroh::EndpointAddr::new(id), id.fmt_short().to_string()),
    }
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    log::info!("tetrix exiting");
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    const FRAME: Duration = Duration::from_millis(16);
    let mut last = Instant::now();
    let mut last_snapshot = Instant::now();
    while !app.quit {
        terminal.draw(|f| ui::draw(f, app))?;
        let wait = FRAME.saturating_sub(last.elapsed());
        if event::poll(wait)? {
            // Drain everything queued so held keys don't lag behind.
            loop {
                if let TermEvent::Key(k) = event::read()? {
                    if k.kind != KeyEventKind::Release {
                        app.on_key(k);
                    }
                }
                if app.quit || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        let now = Instant::now();
        app.update(now - last);
        last = now;

        if log::log_enabled!(log::Level::Debug) && last_snapshot.elapsed() >= Duration::from_millis(500) {
            last_snapshot = Instant::now();
            match &app.game {
                Some(g) if app.screen == Screen::Game => {
                    let p = &g.mat.slots[g.me].player;
                    log::debug!(
                        "snapshot t={}ms me={} piece={:?} next={:?} level={} speed={} lines={} stack={} inv={:?} alive={}/{} paused={}",
                        g.mat.elapsed_ms,
                        g.me,
                        p.piece,
                        p.next,
                        p.level,
                        p.speed_level,
                        p.lines,
                        p.field.stack_height(),
                        p.inventory.iter().map(|s| s.letter()).collect::<String>(),
                        g.mat.alive().count(),
                        g.mat.slots.len(),
                        g.paused
                    );
                }
                _ => log::debug!(
                    "snapshot screen={:?} hosts={} session={:?}",
                    app.screen,
                    app.hosts.len(),
                    app.session.as_ref().map(|s| (&s.status, s.players.len()))
                ),
            }
        }
    }
    Ok(())
}
