//! Local player samples absent from a server's incoming packet stream.
use serde::{Deserialize, Serialize};

/// A post-simulation sample in the same clock domain as the network tape.
/// Inventory, animation and riding state need separate reconstruction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalPlayerFrame {
    pub position: [f64; 3],
    pub previous_position: [f64; 3],
    pub velocity: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
    pub on_ground: bool,
    pub sprinting: bool,
    pub crouching: bool,
    pub swimming: bool,
    pub flying: bool,
    pub fall_flying: bool,
    pub selected: usize,
}

impl LocalPlayerFrame {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self
            .position
            .iter()
            .chain(&self.previous_position)
            .chain(&self.velocity)
            .all(|x| x.is_finite())
            || ![self.yaw, self.pitch, self.head_yaw]
                .iter()
                .all(|x| x.is_finite())
        {
            return Err("Nonfinite local player sample");
        }
        if self.selected >= 9 || !(-90.0..=90.0).contains(&self.pitch) {
            return Err("Invalid local player pitch or hotbar slot");
        }
        Ok(())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 4096 {
            return Err("Local player sample exceeds 4 KiB".into());
        }
        let frame: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        frame.validate()?;
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_untrusted_player_samples() {
        let mut frame = LocalPlayerFrame {
            position: [1.0, 64.0, -2.0],
            previous_position: [0.9, 64.0, -2.0],
            velocity: [0.1, 0.0, 0.0],
            yaw: 180.0,
            pitch: -45.0,
            head_yaw: 175.0,
            on_ground: true,
            sprinting: true,
            crouching: false,
            swimming: false,
            flying: false,
            fall_flying: false,
            selected: 8,
        };
        let bytes = serde_json::to_vec(&frame).unwrap();
        assert_eq!(LocalPlayerFrame::decode(&bytes).unwrap(), frame);
        frame.selected = 9;
        assert!(frame.validate().is_err());
        frame.selected = 0;
        frame.position[0] = f64::NAN;
        assert!(frame.validate().is_err());
        assert!(LocalPlayerFrame::decode(&vec![b' '; 4097]).is_err());
        assert!(LocalPlayerFrame::decode(b"{}").is_err());
    }
}
