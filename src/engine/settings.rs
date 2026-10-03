//! Game settings, mirroring the TetriNET server's "newgame" parameters.

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub initial_level: u32,
    pub lines_per_level: u32,
    pub level_inc: u32,
    /// Every player falls at the average of all levels (TetriNET's
    /// "All Have Players' Averaged Levels").
    pub average_levels: bool,
    /// Lines a player must clear for specials to appear in their field.
    pub special_lines: u32,
    /// Specials placed each time `special_lines` is reached.
    pub special_count: u32,
    pub special_capacity: usize,
    /// Classic multiplayer rules: clearing 2/3/4 lines adds 1/2/4 junk
    /// lines to everyone else.
    pub classic: bool,
    /// Percent frequency per piece, TetriNET order (bar, square, reverse-L,
    /// L, Z, S, T). Must sum to 100.
    pub piece_freq: [u32; 7],
    /// Percent frequency per special, wire order `a c n r s b g q o`.
    /// Must sum to 100.
    pub special_freq: [u32; 9],
    /// Delay before the next piece appears after a lock. Classic TetriNET
    /// uses 600 ms; "tetrifast" servers use 0.
    pub spawn_delay_ms: i64,
    /// Let a hard-dropped piece slide for one gravity step before locking
    /// (tetrinet-0.11's default; the Windows client never slid).
    pub slide: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // Level/special/classic values are tetrinet-0.11's server defaults.
            initial_level: 1,
            lines_per_level: 2,
            level_inc: 1,
            average_levels: true,
            special_lines: 1,
            special_count: 1,
            special_capacity: 18,
            classic: true,
            piece_freq: [14, 14, 15, 14, 14, 14, 15],
            // Jetrix's default occurrences: tetrinet-0.11 ships switch at 0,
            // which takes the most dramatic block out of the game.
            special_freq: [31, 21, 1, 13, 1, 14, 1, 7, 11],
            spawn_delay_ms: 0,
            slide: false,
        }
    }
}

/// Milliseconds per gravity step at `level` (1..=100): 1000 ms at level 1,
/// times 69/70 per level, rounded, exactly as tetrinet-0.11 computes it.
pub fn level_delay(level: u32) -> i64 {
    let mut delay: i64 = 1000;
    for _ in 1..level.clamp(1, 100) {
        delay = (delay * 69 + 35) / 70;
    }
    delay
}

/// Pick an index from a percentage table with a roll in 0..100.
pub fn pick_weighted(freq: &[u32], roll: u32) -> usize {
    let mut n = roll;
    for (i, &f) in freq.iter().enumerate() {
        if n < f {
            return i;
        }
        n -= f;
    }
    freq.len() - 1
}
