//! Wire messages between a Tetrix host and its joiners.
//!
//! Modelled on TetriNET's own protocol: every machine simulates only the
//! players it owns (the host owns itself and any bots, each joiner owns
//! itself) and reports field state; specials and classic lines travel as
//! messages and are applied by whoever owns the target. The host relays
//! every gameplay message from one joiner to all the others, exactly the
//! role the TetriNET server played.
//!
//! Encoding is postcard (serde) inside u32-length-prefixed frames.

use crate::engine::settings::Settings;
use serde::{Deserialize, Serialize};

/// Bumped on any incompatible change; the host rejects mismatched joiners.
pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PieceWire {
    pub kind: u8,
    pub rot: u8,
    pub x: i8,
    pub y: i8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StateWire {
    pub slot: u8,
    /// HEIGHT * WIDTH cells, row-major, TetriNET cell encoding.
    pub field: Vec<u8>,
    pub piece: Option<PieceWire>,
    pub level: u16,
    pub lines: u16,
    pub alive: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LobbyEntry {
    pub name: String,
    pub bot: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Msg {
    /// Joiner's first message.
    Hello { name: String, version: u32 },
    Reject { reason: String },
    /// Who is in the room (host first), resent whenever it changes.
    Lobby { players: Vec<LobbyEntry>, in_game: bool },
    /// A round begins. Everyone builds the same match from the same seed,
    /// so every field gets the same piece sequence.
    Start { round: u32, seed: u64, settings: Settings, players: Vec<LobbyEntry>, you: u8 },
    State(StateWire),
    Special { from: u8, to: u8, special: u8 },
    Lines { from: u8, n: u8 },
    Lost { slot: u8 },
    Over { winner: Option<u8>, eliminated: Vec<u8> },
    /// The host left the round-over screen for the lobby; follow it.
    ToLobby,
    /// Keeps idle lobby connections from timing out.
    Ping,
}

pub fn encode(m: &Msg) -> Vec<u8> {
    postcard::to_allocvec(m).expect("postcard encoding of an owned message cannot fail")
}

pub fn decode(b: &[u8]) -> Option<Msg> {
    postcard::from_bytes(b).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let msgs = vec![
            Msg::Hello { name: "dåvid".into(), version: VERSION },
            Msg::Start {
                round: 3,
                seed: u64::MAX,
                settings: Settings::default(),
                players: vec![LobbyEntry { name: "a".into(), bot: true }],
                you: 1,
            },
            Msg::State(StateWire {
                slot: 2,
                field: vec![7; 264],
                piece: Some(PieceWire { kind: 6, rot: 3, x: -1, y: -2 }),
                level: 100,
                lines: 9,
                alive: true,
            }),
            Msg::Over { winner: None, eliminated: vec![1, 0] },
        ];
        for m in msgs {
            assert_eq!(decode(&encode(&m)), Some(m));
        }
        assert_eq!(decode(&[0xff, 0xff]), None);
    }
}
