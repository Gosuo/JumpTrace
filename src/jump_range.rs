#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumpShipClass {
    JumpFreighterRorqual,
    Caps,
    BlackOps,
    LancerDread,
    CommandCarrier,
    SuperTitan,
}

impl JumpShipClass {
    pub const ALL: [Self; 6] = [
        Self::JumpFreighterRorqual,
        Self::Caps,
        Self::BlackOps,
        Self::LancerDread,
        Self::CommandCarrier,
        Self::SuperTitan,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::JumpFreighterRorqual => "JF/Rorq",
            Self::Caps => "Caps",
            Self::BlackOps => "Black Ops",
            Self::LancerDread => "Lancer Dread",
            Self::CommandCarrier => "Command Carrier",
            Self::SuperTitan => "Super/Titan",
        }
    }

    pub const fn max_range_ly(self) -> f64 {
        match self {
            Self::JumpFreighterRorqual => 10.0,
            Self::BlackOps | Self::LancerDread => 8.0,
            Self::CommandCarrier => 7.5,
            Self::Caps => 7.0,
            Self::SuperTitan => 6.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_assume_jump_drive_calibration_five() {
        assert_eq!(JumpShipClass::JumpFreighterRorqual.max_range_ly(), 10.0);
        assert_eq!(JumpShipClass::Caps.max_range_ly(), 7.0);
        assert_eq!(JumpShipClass::BlackOps.max_range_ly(), 8.0);
        assert_eq!(JumpShipClass::LancerDread.max_range_ly(), 8.0);
        assert_eq!(JumpShipClass::CommandCarrier.max_range_ly(), 7.5);
        assert_eq!(JumpShipClass::SuperTitan.max_range_ly(), 6.0);
    }
}
