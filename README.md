# Tetrix

TetriNET for the terminal: multiplayer Tetris for up to six players, with the classic
special blocks you collect by clearing lines and fire at each other. Part of the
[Asciiverse](https://github.com/theasciiverse) catalog.

```
cargo run --release
```

The menu offers three ways to play:

- **Play solo** against up to five bots (four skill levels).
- **Host a game.** Anyone on the same network picks **Join a game** and sees it listed.
  No addresses to type, and no internet needed. Bots can fill empty seats.
- **Join a game** across networks with `tetrix --join <id>` (the host's lobby shows the id).

Needs a terminal of at least 92x28; bigger terminals get bigger boards.

## Keys

| Key | |
|---|---|
| Left / Right | move |
| Up, x / z | rotate clockwise / counter-clockwise |
| Down / Space | soft drop / hard drop |
| 1-6 | fire your front special at player N (your own number for defensive ones) |
| d | discard the front special |
| s | toggle the landing shadow |
| ? | keys and what every special does |

## Specials

| | | |
|---|---|---|
| A | Add Line | junk line pushed up from the bottom |
| C | Clear Line | removes your bottom row (use on yourself) |
| N | Nuke Field | empties your field (use on yourself) |
| R | Random Clear | knocks out ten random cells |
| S | Switch Fields | swap fields with the target |
| B | Clear Specials | the target's specials turn into plain blocks |
| G | Gravity | every column collapses, filling holes (use on yourself) |
| Q | Blockquake | every row shifts sideways |
| O | Block Bomb | every O block in the target's field explodes |

Classic rules are on: clearing 2, 3 or 4 lines at once adds 1, 2 or 4 junk lines to everyone else.

## Options

`tetrix --help` lists them: `--solo`, `--host`, `--join [id]`, `--bots N`, `--skill N`,
`--name`, `--level N`, `--classic-delay`, `--slide`, `--no-classic`, `--noshadow`, `--seed N`.

Session log for bug reports: `RUST_LOG=debug tetrix 2>tetrix.log`

## Credits

Rules follow St0rmCat's TetriNET 1.13. The piece rotation table and core rules are ported
from Andrew Church's public-domain tetrinet 0.11. Networking uses [iroh](https://iroh.computer).
