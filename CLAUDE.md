# Tetrix

Rust + Ratatui + Drawille.
Spiritual successor to TetriNet — the late 90s multiplayer Tetris that ran on IRC-adjacent
servers. Up to six players, attack blocks, embedded server, LAN autodiscovery, global play.

## Name

Tetrix — Tetris + matrix. Text is a matrix. Clean, short, punchy, original.
Confirmed available: no terminal game, no established open source project, no commercial
title of note uses this name. The only existing "tetrix" is an obscure 2019 Raspberry Pi
LED matrix hardware project — no conflict.

## Premise

TetriNet was the definitive multiplayer Tetris experience of the late 90s. Up to six players
simultaneously, special attack blocks sent to opponents on line clears, genuinely social,
genuinely competitive, genuinely addictive. It deserves a revival — terminal native, Rust,
Drawille rendered, with a multiplayer infrastructure that just works.

## The architectural commitment

Tetrix introduces a new pattern for Asciiverse: the embedded server.

The game binary IS the server. No separate install, no configuration, no IP addresses
typed by hand. One binary does everything:

- **Solo:** just play
- **LAN host:** one player hosts, others on the same network discover automatically
- **LAN join:** `iroh` discovery finds games on the local network instantly
- **Global:** connect to the Asciiverse global server for world play

LAN/WAN autodiscovery via `iroh` (P2P/QUIC — replaces the earlier `mdns-sd`-only
plan with the same local-discovery behavior plus relay-assisted WAN dialing for
free) — the game finds opponents automatically. The moment that works at a LAN
party or scout camp or office, people's eyes go wide.

## Why this matters beyond Tetrix

This is the multiplayer infrastructure pattern for the entire Asciiverse platform.
Get this right once and every subsequent multiplayer title inherits it:
- Antibiosis global network layer
- Bob-sleigh races
- Future titles

Tetrix is the trojan horse for building the Asciiverse multiplayer stack.

## Rendering

Braille sub-pixel rendering for falling pieces — smoother animation than pure cell grid.
Six simultaneous boards visible on screen, each player's field rendered at reduced scale.
Attack garbage lines arriving from opponents as a visual event — colour flash, impact feel.
Ratatui for HUD, score, next piece preview, player list, attack indicators.

## Juice

Terminal games almost never have it, and it's exactly what makes the difference between
a game that feels mechanical and one that feels alive:

- Piece lock impact — screen-shake equivalent in terminal (rapid border flash, colour burst)
- Line clear — the row doesn't just disappear, it explodes outward as Braille particle
  spray before clearing
- Attack blocks arriving — visual shockwave across the recipient's board
- Tetrix (four-line clear) — the whole screen reacts, not just the board
- Level up — colour palette shift, tempo change feels physical
- Combo chain — escalating visual intensity with each consecutive clear

All of this is possible in Ratatui + Drawille and almost nobody is doing it. The juice is
what makes someone record it and post it.

## Game modes

Three modes in one binary:

- **Tetrix** — the TetriNet multiplayer classic, the headline mode. Speed and spatial
  reasoning.
- **Dr Mario** — falling pill capsules, colour matching to clear virus clusters. Same
  engine, different piece logic and win condition. Puzzle solving under pressure.
- **Puyo Puyo** — coloured blob pairs, chain reaction matching, famously deep competitive
  combo system, Sega-published in the West. Chain planning and combo execution — the
  deepest competitive mode of the three, and the juiciest: a four-chain combo with
  Braille particle explosions cascading across the board is the gif that gets posted
  everywhere.

Same embedded server, same LAN autodiscovery, same multiplayer stack across all three.

**Mixed mode:** Puyo Puyo vs Tetrix is a real competitive format that exists in the
official games — worth building into Tetrix as mixed-mode matches where each player
picks their own mode and attacks translate across the different game systems. Genuinely
novel territory, not done elsewhere.

## Game mechanics (TetriNet faithful, Tetrix mode)

- Standard Tetris falling piece mechanics
- Up to six players simultaneously
- Special blocks appear in your field as you clear lines; clearing a line that holds one
  puts it in your inventory (capacity 18). Keys 1-6 fire the front special at that player.
  The real TetriNET 1.13 set is nine blocks (verified against tetrinet.txt and
  tetrinet-0.11, see Prior art below):
  - A Add Line (attack): junk line pushed in from the bottom
  - C Clear Line (defense, use on self): removes your bottom row
  - N Nuke Field (defense, use on self): empties your own field; it is not an attack
  - R Random Clear (attack): empties ten random cells
  - S Switch Fields (attack): swap fields with the target, both stacks lowered to leave 6 clear rows
  - B Clear Specials (attack): target's special blocks become plain blocks
  - G Gravity (defense, use on self): every column collapses, filling holes
  - Q Blockquake (attack): every row shifts 0-3 cells left or right
  - O Block Bomb (attack): every O block in the target's field explodes, scattering its 3x3
- Classic rules (on by default): clearing 2/3/4 lines adds 1/2/4 junk lines to everyone else
- Spectator mode for eliminated players

## Multiplayer architecture

```
tetrix                    # solo play
tetrix --host             # host a LAN game, autodiscovered by others
tetrix --join             # discover and join a LAN game automatically
tetrix --global           # connect to Asciiverse global server
tetrix --join <ip>        # direct connect (fallback)
```

Embedded server: lightweight Tokio async server baked into the binary.
Peer discovery/connection: `iroh` — local LAN discovery and relay-assisted
WAN dialing under one API, no separate mDNS service string to manage.
Global server: hosted by Asciiverse, persistent leaderboards, matchmaking.

## Crate candidates

- `ratatui` — cell grid rendering, HUD, menus
- `drawille` — Braille sub-pixel piece rendering
- `crossterm` — input handling
- `iroh` — LAN + WAN peer discovery and connectivity
- `tokio` — async multiplayer server/client
- `serde` / `bincode` — game state serialisation for network sync

## Open questions / next steps

- [ ] Piece rendering: full Braille sub-pixel or cell-grid blocks? Prototype both.
      Braille gives smoother animation; blocks give crisp classic feel. May be a setting.
- [ ] Network protocol: design the game state sync carefully — Tetris at speed needs
      low-latency reliable ordering, not just UDP fire-and-forget
- [ ] Attack block set: faithful TetriNet set or expand/rebalance for v1?
- [ ] Global server architecture: dedicated Asciiverse server, or peer relay?
- [ ] Leaderboards and persistent identity — ties into Asciiverse account system
- [ ] Spectator mode scope for v1
- [ ] Mobile companion app angle: watch a live Tetrix match on phone?
- [ ] Mixed-mode attack translation — how does a Puyo Puyo chain attack affect a
      Tetrix opponent's board mechanically? Needs its own design pass.

## Prior art (reviewed 2026-10-03)

Archives in `../../references/tetrinet/` (note: `references/`, plural, not the `reference/`
in the top-level CLAUDE.md):

- **tetrinet-0.11** (Andrew Church + Petr Baudis, Linux terminal client + server):
  **public domain**, stated in every source header. The engine ports its rules directly:
  the piece/rotation table is copied verbatim, plus the level-speed formula
  (1000 ms x (69/70)^(level-1)), spawn rules, special-to-inventory rules and server defaults.
  Its `tetrinet.txt` is St0rmCat's original TetriNET 1.13 manual, the rules reference.
- **gtetrinet 0.7.11**, **jetrix 0.2.3**, **tetrinetx 1.13.16**: all **GPLv2**. Used only to
  check behaviour (e.g. how the Windows client did blockquake, add line, switch). No code
  copied. Jetrix's default special frequencies are used as plain numbers.
- **docker-tetrinetx**: a container wrapper around tetrinetx, nothing new.

Useful facts for the network pass: TetriNET is **client-authoritative**. The server only
relays text lines (port 31457, 0xFF-terminated): `f <n> <field diff>`, `sb <to> <special>
<from>`, `lvl`, `playerlost`, `newgame <settings>`. Each client applies specials to its own
field. Field diffs encode cells as `'!'+type` then `'3'+x`, `'3'+y` pairs. `game::Match`
already routes everything through equivalent calls, so the iroh layer slots in there.

## Code layout

```
src/engine/   pure rules, no terminal or network: piece.rs (TetriNET shape table),
              field.rs (12x22, TetriNET cell encoding), special.rs (the nine specials),
              player.rs (one player's game, tick(dt) + event outbox), settings.rs
src/game.rs   Match: up to 6 slots, each owned (simulated here) or a mirror of a remote
              player; routes specials/classic lines, level averaging, winner; outbox of
              wire messages + apply_remote() for the other direction
src/bot.rs    computer opponents: 4-feature placement heuristic, key-by-key moves, special tactics
src/proto.rs  wire messages (serde + postcard, u32 length-prefixed frames)
src/lan.rs    LAN discovery beacon (UDP broadcast on 31457, TetriNET's old port)
src/net.rs    iroh endpoint on a background tokio thread, bridged to the game loop by
              non-blocking channels (same shape as games/mato's net/bridge.rs)
src/fx.rs     juice: Braille line-clear spray, lock/attack border flashes, shake, banners
src/ui.rs     every screen: title menu, LAN browser, lobby, game; game layout scales
src/main.rs   App state machine (Menu -> Browse/Lobby -> Game), host relay, input, winlist,
              session logging (RUST_LOG=debug tetrix 2>tetrix.log)
```

## Multiplayer (built 2026-10-03)

Star topology like TetriNET: the host is the relay. Each machine simulates only the players
it owns (the host owns itself and any bots, each joiner owns itself) and sends `State` on
every field change plus piece movement every 100 ms. `Special` / `Lines` / `Lost` are applied
by whoever owns the target; the host forwards every joiner message to the others and rejects
messages about slots the sender doesn't own. The host alone decides the round (`Over`).
Everyone builds the match from the same seed, so all fields get the same piece sequence.

Discovery: iroh can only dial a known `EndpointId` (no browse API, found in mato's spike), so
the host broadcasts `TETRIX-LAN-1|id|ports|players|in_game|name` every 0.8 s to
255.255.255.255 and each private interface's /24 broadcast. The joiner pairs the packet's
source IP with the listed iroh ports, so joining works on a LAN with no internet at all
(the host never waits on `online()`). With internet, `tetrix --join <id>` works across
networks via iroh's relay and lookup. QUIC idle timeout is set to 6 s (both sides ping every
2 s), so a crashed player drops out in ~6 s instead of iroh's default ~25 s.

Verified live 2026-10-03 with three processes on one machine over real iroh connections:
browser finds the host, lobby syncs, round starts on all, fields mirror exactly (compared cell
by cell), specials and classic lines cross machines within ~10 ms, round result and winlist
on every machine, back-to-lobby and round 2, killed joiner dropped after 6 s. Not yet tried
across two physical machines or across the internet.

Rendering decision for now: cell-grid blocks for your field (2x1 terminal cells per field cell,
growing to 4x2 or 6x3 when the terminal has room), opponents as half-block minis (12x11) or
full-size boards when they fit, and Braille only for particles. A 12x22 field
in Braille is just 6x6 terminal cells, too small to read. The `drawille` crate isn't used: the
Braille encoding is ~10 lines and writes straight into the ratatui buffer.

Deliberate rule choices (all in `Settings`, all flippable):
- Hard drop locks instantly (Windows TetriNET); `--slide` gives tetrinet-0.11's slide window.
- No spawn delay ("tetrifast"); `--classic-delay` restores the 600 ms pause.
- Shadow/ghost piece on (tetrinet-0.11 default, not in Windows TetriNET); `s` toggles.
- Rotation blocked only by a wall nudges the piece back inside (gtetrinet behaviour), no SRS kicks.
- Same piece sequence for every player (TetriNET 1.14 "same blocks").

## Status

Playable (2026-10-03): title menu, solo against 0-5 bots (`--skill 0-3`), LAN multiplayer for
up to six with automatic discovery and a lobby (bots can fill empty seats), full special set,
classic lines, level averaging, winlist across rounds, juice pass, layout that scales with the
terminal. 14 tests, including six headless 6-bot matches and a 6-player match split across
three simulated machines that must stay cell-for-cell in sync.
Not built yet: partyline/chat, teams, Dr Mario / Puyo Puyo modes, sudden death, WAN game
browsing (only direct `--join <id>` across networks).

Version 0.6.0 (2026-10-28). What comes next, release by release, is in `ROADMAP.md`.

Name confirmed: Tetrix. Conceived 2026-07-02, scout camp.
Multiplayer infrastructure built here powers future Asciiverse titles.
