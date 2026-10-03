//! Every screen: title menu, LAN browser, lobby, and the game.
//!
//! In game, your field is drawn big on the left, the HUD in the middle and
//! every opponent on the right. Everything scales with the terminal: your
//! field grows to 2x or 3x cells, and opponents switch from half-block
//! minis (one terminal row per two field rows) to full-size boards when
//! there is room.

use crate::engine::field::{Field, EMPTY, HEIGHT, SPECIAL_BASE, WIDTH};
use crate::engine::piece::PieceKind;
use crate::engine::player::Player;
use crate::engine::special::Special;
use crate::{App, Game, MenuItem, Screen, MENU, SKILL_NAMES};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};
use ratatui::Frame;

const GREEN: Color = Color::Rgb(0, 200, 90);
const DARK_GREEN: Color = Color::Rgb(0, 130, 60);
const AMBER: Color = Color::Rgb(255, 176, 0);
const DIM: Color = Color::Rgb(90, 90, 100);
const TEXT: Color = Color::Rgb(200, 200, 205);
const BG: Color = Color::Rgb(10, 10, 14);
const GRID: Color = Color::Rgb(34, 34, 44);
const RED: Color = Color::Rgb(255, 90, 90);

const HUD_W: u16 = 26;

pub fn cell_color(c: u8) -> Color {
    match c {
        1 => Color::Rgb(60, 120, 255),
        2 => Color::Rgb(240, 210, 40),
        3 => Color::Rgb(60, 200, 80),
        4 => Color::Rgb(175, 85, 225),
        5 => Color::Rgb(235, 60, 60),
        c if c >= SPECIAL_BASE => Color::Rgb(235, 235, 235),
        _ => BG,
    }
}

fn put(buf: &mut Buffer, x: i32, y: i32, s: &str, style: Style) {
    let area = buf.area;
    if x < area.left() as i32 || y < area.top() as i32 || y >= area.bottom() as i32 {
        return;
    }
    buf.set_stringn(x as u16, y as u16, s, (area.right() as i32 - x).max(0) as usize, style);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let (w, h) = (w.min(area.width), h.min(area.height));
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(BG)), area);
    match app.screen {
        Screen::Menu => draw_menu(f, app, area),
        Screen::Browse => draw_browse(f, app, area),
        Screen::Lobby => draw_lobby(f, app, area),
        Screen::Game => {
            if let Some(g) = &app.game {
                draw_game(f, app, g, area);
            }
        }
    }
    if app.help {
        draw_help(f, area);
    }
    if let Some((msg, _)) = &app.toast {
        let w = msg.chars().count() as u16 + 4;
        let r = Rect::new(area.x + area.width.saturating_sub(w) / 2, area.bottom().saturating_sub(2), w.min(area.width), 1);
        f.render_widget(Clear, r);
        f.render_widget(Paragraph::new(Span::styled(format!("  {msg}  "), Style::new().fg(Color::Black).bg(AMBER))), r);
    }
}

// ---- title menu -------------------------------------------------------------

const LOGO: [&str; 2] = ["▀█▀ █▀▀ ▀█▀ █▀█ █ ▀▄▀", " █  ██▄  █  █▀▄ █ ▄▀▄"];
/// Column where each of the six letters starts in `LOGO`.
const LOGO_LETTERS: [usize; 6] = [0, 4, 8, 12, 16, 18];

/// The logo with each letter in a TetriNET block colour.
fn logo_lines() -> Vec<Line<'static>> {
    LOGO.iter()
        .map(|row| {
            let chars: Vec<char> = row.chars().collect();
            let spans = LOGO_LETTERS
                .iter()
                .enumerate()
                .map(|(k, &start)| {
                    let end = LOGO_LETTERS.get(k + 1).copied().unwrap_or(chars.len()).min(chars.len());
                    let s: String = chars[start..end].iter().collect();
                    Span::styled(s, Style::new().fg(cell_color((k % 5) as u8 + 1)).add_modifier(Modifier::BOLD))
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect()
}

fn draw_menu(f: &mut Frame, app: &App, area: Rect) {
    let r = centered(area, 52, 22);
    let mut lines = logo_lines();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("TetriNET for the terminal. Up to six players.", Style::new().fg(DIM))));
    lines.push(Line::from(""));
    for (k, item) in MENU.iter().enumerate() {
        let sel = k == app.menu_sel;
        let marker = if sel { "▶ " } else { "  " };
        let style = if sel { Style::new().fg(AMBER).add_modifier(Modifier::BOLD) } else { Style::new().fg(TEXT) };
        let adjust = |label: &str, v: String| {
            let v = if sel { format!("‹ {v} ›") } else { format!("  {v}  ") };
            Line::from(vec![Span::styled(format!("{marker}{label:<10}"), style), Span::styled(v, Style::new().fg(GREEN))])
        };
        let hint = |label: &str, h: String| {
            Line::from(vec![Span::styled(format!("{marker}{label:<13}"), style), Span::styled(h, Style::new().fg(DIM))])
        };
        let line = match item {
            MenuItem::Solo => hint("Play solo", format!("vs {} bot{}", app.bots, if app.bots == 1 { "" } else { "s" })),
            MenuItem::Host => hint("Host a game", "others on your network see it".into()),
            MenuItem::Join => hint("Join a game", "find games on your network".into()),
            MenuItem::Name if app.editing_name => Line::from(vec![
                Span::styled(format!("{marker}{:<10}", "Name"), style),
                Span::styled(format!(" {}▏", app.name), Style::new().fg(Color::Black).bg(GREEN)),
            ]),
            MenuItem::Name => adjust("Name", app.name.clone()),
            MenuItem::Bots => adjust("Bots", app.bots.to_string()),
            MenuItem::Skill => adjust("Bot skill", SKILL_NAMES[app.skill as usize].to_string()),
            MenuItem::Quit => Line::from(Span::styled(format!("{marker}Quit"), style)),
        };
        lines.push(line);
        if matches!(item, MenuItem::Join | MenuItem::Skill) {
            lines.push(Line::from(""));
        }
    }
    lines.push(Line::from(Span::styled(
        if app.editing_name { "type a name, Enter to keep it" } else { "↑↓ choose   Enter select   ←→ adjust   ? keys" },
        Style::new().fg(DIM),
    )));
    f.render_widget(Paragraph::new(lines), r);
}

// ---- LAN browser -----------------------------------------------------------

fn spinner() -> char {
    const S: [char; 8] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧'];
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    S[(t / 100) as usize % S.len()]
}

fn draw_browse(f: &mut Frame, app: &App, area: Rect) {
    let r = centered(area, 66, 18);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(GREEN))
        .title(Span::styled(" Games on your network ", Style::new().fg(AMBER).add_modifier(Modifier::BOLD)));
    let inner = block.inner(r);
    f.render_widget(block, r);
    let mut lines = vec![Line::from("")];
    if app.hosts.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", spinner()), Style::new().fg(AMBER)),
            Span::styled("Looking for games...", Style::new().fg(TEXT)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(" When someone on the same network picks \"Host a game\",", Style::new().fg(DIM))));
        lines.push(Line::from(Span::styled(" it shows up here by itself.", Style::new().fg(DIM))));
    } else {
        for (k, h) in app.hosts.iter().enumerate() {
            let sel = k == app.browse_sel;
            let style = if sel { Style::new().fg(Color::Black).bg(AMBER) } else { Style::new().fg(TEXT) };
            let state = if h.beacon.in_game { "playing" } else { "in lobby" };
            let name: String = h.beacon.name.chars().take(24).collect();
            lines.push(Line::from(vec![
                Span::styled(format!(" {} {name:<24}", if sel { "▶" } else { " " }), style),
                Span::styled(format!("  {}/6  ", h.beacon.players), Style::new().fg(GREEN)),
                Span::styled(format!("{state:<9}"), Style::new().fg(if h.beacon.in_game { AMBER } else { DIM })),
                Span::styled(format!(" {}", h.from), Style::new().fg(DIM)),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", spinner()), Style::new().fg(DIM)),
            Span::styled("still listening", Style::new().fg(DIM)),
        ]));
    }
    f.render_widget(Paragraph::new(lines), inner);
    f.render_widget(
        Paragraph::new(Span::styled("↑↓ choose   Enter join   Esc back", Style::new().fg(DIM))).alignment(Alignment::Center),
        Rect::new(r.x, r.bottom().min(area.bottom().saturating_sub(1)), r.width, 1),
    );
}

// ---- lobby -----------------------------------------------------------------

fn draw_lobby(f: &mut Frame, app: &App, area: Rect) {
    let Some(s) = &app.session else { return };
    let r = centered(area, 84, 20);
    let title = if s.host { format!(" {} (you're hosting) ", s.host_name) } else { format!(" {} ", s.host_name) };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(GREEN))
        .title(Span::styled(title, Style::new().fg(AMBER).add_modifier(Modifier::BOLD)));
    let inner = block.inner(r);
    f.render_widget(block, r);

    let ok = s.connected && !s.status.starts_with("Network problem") && !s.status.starts_with("Opening");
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {} ", if ok { '●' } else { spinner() }), Style::new().fg(if ok { GREEN } else { AMBER })),
            Span::styled(s.status.clone(), Style::new().fg(TEXT)),
        ]),
        Line::from(""),
    ];
    // The joiner can't know their own slot until a round starts, so "you"
    // is matched by name (good enough for a lobby label).
    let mut you_marked = false;
    for slot in 0..6 {
        let mut spans = vec![Span::styled(format!("  {}  ", slot + 1), Style::new().fg(DIM))];
        match s.players.get(slot) {
            Some(p) => {
                spans.push(Span::styled("██ ", Style::new().fg(cell_color((slot % 5) as u8 + 1))));
                spans.push(Span::styled(format!("{:<12}", p.name), Style::new().fg(TEXT).add_modifier(Modifier::BOLD)));
                let mut tags = Vec::new();
                if slot == 0 {
                    tags.push("host");
                }
                if p.bot {
                    tags.push("bot");
                }
                let is_you = if s.host { slot == 0 } else { slot > 0 && !p.bot && p.name == app.name };
                if is_you && !you_marked {
                    you_marked = true;
                    tags.push("you");
                }
                spans.push(Span::styled(tags.join(", "), Style::new().fg(DIM)));
            }
            None => spans.push(Span::styled("·· open", Style::new().fg(GRID))),
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(""));
    let key = |k: &str| Span::styled(k.to_string(), Style::new().fg(AMBER));
    let txt = |t: String| Span::styled(t, Style::new().fg(DIM));
    if s.host {
        let bots = s.players.iter().filter(|p| p.bot).count();
        lines.push(Line::from(vec![
            txt("  ".into()),
            key("Enter"),
            txt(" start   ".into()),
            key("←→"),
            txt(format!(" bots ({bots})   ")),
            key("s"),
            txt(format!(" bot skill ({})   ", SKILL_NAMES[app.skill as usize])),
            key("Esc"),
            txt(" close".into()),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(txt("  Same network: they pick \"Join a game\" and see this game listed.".into())));
        if let Some(id) = &s.endpoint {
            lines.push(Line::from(txt("  From anywhere else (needs internet), they run:".into())));
            lines.push(Line::from(Span::styled(format!(" tetrix --join {id}"), Style::new().fg(DARK_GREEN))));
        }
    } else {
        lines.push(Line::from(vec![txt("  The host starts the round.   ".into()), key("Esc"), txt(" leave".into())]));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

// ---- game ------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
struct Layout {
    /// Field cell size on your board: 2*s columns by s rows.
    s: u16,
    /// Opponent boards at full size (2x1 cells) instead of half-block minis.
    full: bool,
    cols: u16,
    rows: u16,
}

impl Layout {
    fn main_w(self) -> u16 {
        WIDTH as u16 * 2 * self.s + 2
    }
    fn main_h(self) -> u16 {
        HEIGHT as u16 * self.s + 2
    }
    fn mini_w(self) -> u16 {
        if self.full {
            WIDTH as u16 * 2 + 2
        } else {
            WIDTH as u16 + 2
        }
    }
    fn mini_h(self) -> u16 {
        if self.full {
            HEIGHT as u16 + 2
        } else {
            HEIGHT as u16 / 2 + 2
        }
    }
    fn width(self) -> u16 {
        self.main_w() + 1 + HUD_W + if self.cols > 0 { 1 + self.cols * self.mini_w() } else { 0 }
    }
    fn height(self) -> u16 {
        1 + (self.main_h() + 3).max(self.rows * (self.mini_h() + 1))
    }

    /// The biggest arrangement that fits: your board first, then
    /// full-size opponents, then fewest rows of opponents.
    fn fit(area: Rect, opponents: u16) -> Option<Layout> {
        for s in (1..=3).rev() {
            for full in [true, false] {
                for cols in (opponents.min(1)..=opponents).rev() {
                    let rows = if cols == 0 { 0 } else { opponents.div_ceil(cols) };
                    let l = Layout { s, full, cols, rows };
                    if l.width() <= area.width && l.height() <= area.height {
                        return Some(l);
                    }
                }
            }
        }
        None
    }
}

fn draw_game(f: &mut Frame, app: &App, g: &Game, area: Rect) {
    let opponents = g.mat.slots.len().saturating_sub(1) as u16;
    let Some(lay) = Layout::fit(area, opponents) else {
        let min = Layout { s: 1, full: false, cols: opponents.min(3), rows: opponents.div_ceil(3) };
        let msg =
            format!("Tetrix needs a {}x{} terminal (this one is {}x{})", min.width(), min.height(), area.width, area.height);
        f.render_widget(Paragraph::new(msg).alignment(Alignment::Center).wrap(Wrap { trim: true }), area);
        return;
    };

    let (w, h) = (lay.width(), lay.height());
    let x0 = area.x + (area.width - w) / 2;
    let y0 = area.y + (area.height - h) / 2;

    let mut title = vec![
        Span::styled(" TETRIX ", Style::new().fg(Color::Black).bg(GREEN).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  round {}  ", g.round), Style::new().fg(DIM)),
    ];
    if let Some(s) = &app.session {
        title.push(Span::styled(if s.host { "hosting  " } else { "online  " }, Style::new().fg(DARK_GREEN)));
    }
    title.push(Span::styled("?", Style::new().fg(AMBER)));
    title.push(Span::styled(" help", Style::new().fg(DIM)));
    f.render_widget(Paragraph::new(Line::from(title)), Rect::new(x0, y0, w, 1));

    let main = Rect::new((x0 as i32 + g.fx.shake_offset()).max(0) as u16, y0 + 1, lay.main_w(), lay.main_h());
    draw_main(f, app, g, main, lay.s);
    draw_inventory(f, &g.mat.slots[g.me].player, Rect::new(x0, y0 + 1 + lay.main_h(), lay.main_w(), 3));
    draw_hud(f, g, Rect::new(x0 + lay.main_w() + 1, y0 + 1, HUD_W, lay.main_h() + 3));

    let others: Vec<usize> = (0..g.mat.slots.len()).filter(|&i| i != g.me).collect();
    for (k, &i) in others.iter().enumerate() {
        let k = k as u16;
        let r = Rect::new(
            x0 + lay.main_w() + 1 + HUD_W + 1 + (k % lay.cols) * lay.mini_w(),
            y0 + 1 + (k / lay.cols) * (lay.mini_h() + 1),
            lay.mini_w(),
            lay.mini_h(),
        );
        draw_opponent(f, g, i, r, lay.full);
    }

    g.fx.render_particles((main.x as i32 + 1, main.y as i32 + 1), lay.s as f32, area, f.buffer_mut());

    if g.fx.screen_flash > 0.0 {
        let buf = f.buffer_mut();
        for x in area.left()..area.right() {
            for y in [area.top(), area.bottom() - 1] {
                if let Some(c) = buf.cell_mut((x, y)) {
                    c.set_char('▀').set_fg(AMBER);
                }
            }
        }
    }

    if let Some((text, color, _)) = &g.fx.banner {
        let bw = text.chars().count() as u16 + 4;
        let r = Rect::new(main.x + lay.main_w().saturating_sub(bw) / 2, main.y + lay.main_h() / 3, bw.min(lay.main_w()), 3);
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(Span::styled(text.as_str(), Style::new().fg(*color).add_modifier(Modifier::BOLD)))
                .alignment(Alignment::Center)
                .block(Block::bordered().border_type(BorderType::Double).border_style(Style::new().fg(*color))),
            r,
        );
    }

    if g.mat.outcome.is_some() {
        draw_game_over(f, app, g, area);
    } else if !g.mat.slots[g.me].player.alive {
        let hint =
            if g.networked { "you're out: watching   q: back to lobby" } else { "you're out: watching   Enter: next round" };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, Style::new().fg(AMBER)))).alignment(Alignment::Center),
            Rect::new(area.x, area.bottom() - 1, area.width, 1),
        );
    } else if g.paused {
        popup(f, area, " Paused ", vec![Line::from("p to resume")], 30, 1);
    }
}

/// Draw a field (with an optional falling piece and its ghost) with each
/// cell `2*s` columns by `s` rows. Above 1x, blocks get a dark right and
/// bottom edge so neighbouring blocks read as separate tiles.
#[allow(clippy::too_many_arguments)]
fn draw_board(
    buf: &mut Buffer,
    field: &Field,
    ox: i32,
    oy: i32,
    s: u16,
    dim: bool,
    piece: Option<(PieceKind, usize, i32, i32)>,
    ghost_y: Option<i32>,
) {
    let s = s as i32;
    let cw = 2 * s;
    let tint = |c: u8| if dim && c != EMPTY { Color::Rgb(45, 45, 52) } else { cell_color(c) };
    let block = |buf: &mut Buffer, x: i32, y: i32, color: Color| {
        for r in 0..s {
            let row = if s == 1 {
                "██".to_string()
            } else if r < s - 1 {
                format!("{}▌", "█".repeat(cw as usize - 1))
            } else {
                format!("{}▘", "▀".repeat(cw as usize - 1))
            };
            put(buf, ox + x * cw, oy + y * s + r, &row, Style::new().fg(color).bg(BG));
        }
    };
    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            let c = field.get(x as usize, y as usize);
            if c == EMPTY {
                for r in 0..s {
                    let row: String = (0..cw).map(|k| if r == s / 2 && k == cw - 1 { '·' } else { ' ' }).collect();
                    put(buf, ox + x * cw, oy + y * s + r, &row, Style::new().fg(GRID).bg(BG));
                }
            } else if let (Some(sp), false) = (Special::from_cell(c), dim) {
                for r in 0..s {
                    let mut row = vec![' '; cw as usize];
                    if r == s / 2 {
                        row[(cw as usize - 1) / 2] = sp.letter().to_ascii_uppercase();
                    }
                    let row: String = row.into_iter().collect();
                    let style = Style::new().fg(Color::Black).bg(cell_color(c)).add_modifier(Modifier::BOLD);
                    put(buf, ox + x * cw, oy + y * s + r, &row, style);
                }
            } else {
                block(buf, x, y, tint(c));
            }
        }
    }
    if let Some((kind, rot, px, py)) = piece {
        if let Some(gy) = ghost_y.filter(|&gy| gy != py) {
            for (x, y) in kind.data(rot).blocks(px, gy) {
                if y >= 0 {
                    for r in 0..s {
                        let style = Style::new().fg(Color::Rgb(70, 70, 85)).bg(BG);
                        put(buf, ox + x * cw, oy + y * s + r, &"░".repeat(cw as usize), style);
                    }
                }
            }
        }
        let color = cell_color(kind.color());
        for (x, y) in kind.data(rot).blocks(px, py) {
            if (0..HEIGHT as i32).contains(&y) {
                block(buf, x, y, color);
            }
        }
    }
}

fn draw_main(f: &mut Frame, app: &App, g: &Game, r: Rect, s: u16) {
    let p = &g.mat.slots[g.me].player;
    let border = g.fx.border_flash.map_or(GREEN, |(c, _)| c);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .title(Span::styled(format!(" {} {} ", g.me + 1, g.mat.slots[g.me].name), Style::new().fg(AMBER)));
    let inner = block.inner(r);
    f.render_widget(block, r);
    let buf = f.buffer_mut();
    let (ox, oy) = (inner.x as i32, inner.y as i32);
    let piece = p.piece.map(|a| (a.kind, a.rot, a.x, a.y));
    let ghost = if app.shadow { p.ghost_y() } else { None };
    draw_board(buf, &p.field, ox, oy, s, false, piece, ghost);
    for &(y, _) in &g.fx.row_flash {
        for k in 0..s as i32 {
            put(buf, ox, oy + y as i32 * s as i32 + k, &"█".repeat(WIDTH * 2 * s as usize), Style::new().fg(Color::White));
        }
    }
}

fn draw_opponent(f: &mut Frame, g: &Game, i: usize, r: Rect, full: bool) {
    let slot = &g.mat.slots[i];
    let p = &slot.player;
    let border = if g.fx.mini_flash[i] > 0.0 {
        RED
    } else if p.alive {
        DARK_GREEN
    } else {
        DIM
    };
    let label = if p.alive { format!(" {} {} ", i + 1, slot.name) } else { format!(" {} out ", i + 1) };
    let block = Block::bordered()
        .border_style(Style::new().fg(border))
        .title(Span::styled(label, Style::new().fg(if p.alive { AMBER } else { DIM })))
        .title_bottom(Line::from(Span::styled(format!(" L{} ", p.level), Style::new().fg(DIM))).right_aligned());
    let inner = block.inner(r);
    f.render_widget(block, r);
    let buf = f.buffer_mut();

    if full {
        let piece = p.piece.map(|a| (a.kind, a.rot, a.x, a.y));
        draw_board(buf, &p.field, inner.x as i32, inner.y as i32, 1, !p.alive, piece, None);
        return;
    }
    // Half-block mini: overlay the falling piece so the field feels live.
    let mut field = p.field.clone();
    if let Some(a) = p.piece {
        for (x, y) in a.kind.data(a.rot).blocks(a.x, a.y) {
            if (0..HEIGHT as i32).contains(&y) && (0..WIDTH as i32).contains(&x) {
                field.set(x as usize, y as usize, a.kind.color());
            }
        }
    }
    let shade = |c: u8| {
        if p.alive {
            cell_color(c)
        } else if c == EMPTY {
            BG
        } else {
            Color::Rgb(45, 45, 52)
        }
    };
    for row in 0..HEIGHT / 2 {
        for x in 0..WIDTH {
            let top = field.get(x, row * 2);
            let bot = field.get(x, row * 2 + 1);
            put(buf, inner.x as i32 + x as i32, inner.y as i32 + row as i32, "▀", Style::new().fg(shade(top)).bg(shade(bot)));
        }
    }
}

fn draw_inventory(f: &mut Frame, p: &Player, r: Rect) {
    let mut spans = Vec::new();
    for (k, s) in p.inventory.iter().enumerate() {
        let style = if k == 0 {
            Style::new().fg(Color::Black).bg(Color::Rgb(235, 60, 60)).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Rgb(220, 220, 220))
        };
        spans.push(Span::styled(s.letter().to_ascii_uppercase().to_string(), style));
    }
    let cap = p.settings.special_capacity;
    let block = Block::bordered()
        .border_style(Style::new().fg(DARK_GREEN))
        .title(Span::styled(format!(" specials {}/{} ", p.inventory.len(), cap), Style::new().fg(DIM)));
    f.render_widget(Paragraph::new(Line::from(spans)).block(block), r);
}

fn draw_hud(f: &mut Frame, g: &Game, r: Rect) {
    let p = &g.mat.slots[g.me].player;
    let mut lines = vec![Line::from(Span::styled("NEXT", Style::new().fg(DIM)))];
    lines.extend(next_piece_lines(p.next));
    lines.push(Line::from(""));
    let stat = |k: &str, v: String| {
        Line::from(vec![Span::styled(format!("{k:<7}"), Style::new().fg(DIM)), Span::styled(v, Style::new().fg(GREEN))])
    };
    lines.push(stat("level", format!("{} (speed {})", p.level, p.speed_level)));
    lines.push(stat("lines", p.lines.to_string()));
    lines.push(stat("alive", format!("{}/{}", g.mat.alive().count(), g.mat.slots.len())));
    lines.push(Line::from(""));
    match p.inventory.front() {
        Some(s) => {
            let (what, color) = if s.is_defensive() {
                (format!("use on self: {}", g.me + 1), GREEN)
            } else {
                ("attack: their number".to_string(), RED)
            };
            lines.push(Line::from(Span::styled(s.name(), Style::new().fg(AMBER).add_modifier(Modifier::BOLD))));
            lines.push(Line::from(Span::styled(what, Style::new().fg(color))));
        }
        None => {
            lines.push(Line::from(Span::styled("no special", Style::new().fg(DIM))));
            lines.push(Line::from(""));
        }
    }
    lines.push(Line::from(""));
    // Newest messages last, word-wrapped to the HUD width, as many as fit.
    let room = (r.height as usize).saturating_sub(lines.len() + 2);
    let width = r.width.saturating_sub(2) as usize;
    let mut log_lines: Vec<String> = Vec::new();
    for msg in g.mat.log.iter().rev() {
        let wrapped = wrap(msg, width);
        if log_lines.len() + wrapped.len() > room {
            break;
        }
        log_lines.splice(0..0, wrapped);
    }
    for l in log_lines {
        lines.push(Line::from(Span::styled(l, Style::new().fg(Color::Rgb(160, 160, 170)))));
    }
    let block = Block::bordered().border_style(Style::new().fg(DARK_GREEN));
    f.render_widget(Paragraph::new(lines).block(block), r);
}

/// Greedy word wrap; continuation lines are indented two spaces so
/// separate messages stay distinguishable.
fn wrap(msg: &str, width: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in msg.split_whitespace() {
        let indent = if out.is_empty() { 0 } else { 2 };
        if !cur.is_empty() && indent + cur.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.into_iter().enumerate().map(|(k, l)| if k == 0 { l } else { format!("  {l}") }).collect()
}

fn next_piece_lines(kind: PieceKind) -> Vec<Line<'static>> {
    let d = kind.data(0);
    let color = cell_color(kind.color());
    (d.top..=d.bottom)
        .map(|y| {
            let s: String =
                (0..4).map(|x| if d.cells[y as usize][x] { "██" } else { "  " }).collect::<Vec<_>>().concat();
            Line::from(Span::styled(format!("  {s}"), Style::new().fg(color)))
        })
        .chain(std::iter::repeat(Line::from("")).take(2 - (d.bottom - d.top).min(1) as usize))
        .collect()
}

fn popup(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line>, w: u16, h: u16) {
    let r = centered(area, w, h + 2);
    f.render_widget(Clear, r);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::new().fg(AMBER))
        .title(Span::styled(title.to_string(), Style::new().fg(AMBER)))
        .style(Style::new().bg(BG));
    f.render_widget(Paragraph::new(lines).block(block), r);
}

fn draw_game_over(f: &mut Frame, app: &App, g: &Game, area: Rect) {
    let o = g.mat.outcome.as_ref().unwrap();
    let mut lines = Vec::new();
    let headline = match o.winner {
        Some(w) if w == g.me => "You win!".to_string(),
        Some(w) => format!("{} wins", g.mat.slots[w].name),
        None => format!("Game over: {} lines", g.mat.slots[g.me].player.lines),
    };
    lines.push(Line::from(Span::styled(headline, Style::new().fg(AMBER).add_modifier(Modifier::BOLD))));
    lines.push(Line::from(""));
    if !app.winlist.is_empty() {
        lines.push(Line::from(Span::styled("Winlist (3/2/1 points)", Style::new().fg(DIM))));
        for (name, pts) in app.winlist.iter().take(6) {
            lines.push(Line::from(Span::styled(format!("  {pts:>3}  {name}"), Style::new().fg(GREEN))));
        }
        lines.push(Line::from(""));
    }
    let keys = if !g.networked {
        "Enter: next round   q: menu"
    } else if g.is_host {
        "Enter: next round   l: lobby"
    } else {
        "waiting for the host   q: lobby"
    };
    lines.push(Line::from(Span::styled(keys, Style::new().fg(DIM))));
    let h = lines.len() as u16;
    popup(f, area, " Round over ", lines, 36, h);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let k = |key: &str, what: &str| {
        Line::from(vec![Span::styled(format!("{key:<14}"), Style::new().fg(AMBER)), Span::raw(what.to_string())])
    };
    let mut lines = vec![
        k("Left / Right", "move"),
        k("Up  x", "rotate clockwise"),
        k("z", "rotate counter-clockwise"),
        k("Down", "soft drop"),
        k("Space", "hard drop"),
        k("1-6", "use front special on player N"),
        k("d", "discard front special"),
        k("s", "toggle piece shadow"),
        k("p", "pause (solo only)"),
        k("q  Esc", "leave"),
        Line::from(""),
    ];
    for s in Special::ALL {
        let tag = if s.is_defensive() { "self" } else { "attack" };
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", s.letter().to_ascii_uppercase()), Style::new().fg(Color::Black).bg(Color::Rgb(235, 235, 235))),
            Span::raw(format!(" {:<15}", s.name())),
            Span::styled(tag, Style::new().fg(if s.is_defensive() { GREEN } else { RED })),
        ]));
    }
    let h = lines.len() as u16;
    popup(f, area, " Keys & specials (? closes) ", lines, 46, h);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_grows_with_the_terminal() {
        let small = Layout::fit(Rect::new(0, 0, 100, 30), 5).unwrap();
        assert_eq!((small.s, small.full), (1, false));
        let big = Layout::fit(Rect::new(0, 0, 175, 52), 5).unwrap();
        assert!(big.s >= 2, "{big:?}");
        let huge = Layout::fit(Rect::new(0, 0, 260, 75), 5).unwrap();
        assert_eq!(huge.s, 3, "{huge:?}");
        assert!(Layout::fit(Rect::new(0, 0, 60, 20), 5).is_none());
    }
}
