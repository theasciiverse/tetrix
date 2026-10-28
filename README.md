# Tetrix

**SIX FIELDS. NINE DIRTY TRICKS. ONE SURVIVOR.**

The late-90s LAN-party block buster legend TetriNET, reborn in your terminal! Up to six
players drop blocks side by side, clear lines to collect specials, and fire
them at each other: junk lines, Blockquakes, Block Bombs and the infamous
Switch Fields. Host on your network and the game simply appears on everyone's
menu, no setup at all. Or take on the bots, if you dare face Dvorak on brutal.

## Play

With [av](https://asciiverse.sh), the Asciiverse client:

```
av install tetrix
av play tetrix
```

To build it yourself: `cargo run --release`.

Two ways to play, from the menu:

- **Play solo** against up to five bots (four skill levels).
- **Host a game.** Anyone on the same network picks **Join a game** and sees it listed.
  No addresses to type, and no internet needed. Bots can fill empty seats.

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

## Licence

GNU General Public License, version 2 or (at your option) any later
version; see [LICENSE](LICENSE).
