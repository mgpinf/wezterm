use std::convert::{TryFrom, TryInto};
use std::num::NonZeroU32;

const FRACTIONAL_SCALE_DENOMINATOR: u32 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WaylandScale {
    numerator: NonZeroU32,
}

impl WaylandScale {
    pub(crate) const ONE: Self = Self {
        numerator: NonZeroU32::new(FRACTIONAL_SCALE_DENOMINATOR).unwrap(),
    };

    pub(crate) fn from_numerator(numerator: u32) -> Option<Self> {
        Some(Self {
            numerator: NonZeroU32::new(numerator)?,
        })
    }

    pub(crate) fn from_integer(factor: i32) -> Option<Self> {
        let factor = u32::try_from(factor).ok()?;
        Self::from_numerator(factor.checked_mul(FRACTIONAL_SCALE_DENOMINATOR)?)
    }

    pub(crate) fn from_factor(factor: f64) -> Option<Self> {
        let numerator = factor * f64::from(FRACTIONAL_SCALE_DENOMINATOR);
        if !numerator.is_finite() || numerator < 1.0 || numerator > f64::from(u32::MAX) {
            return None;
        }
        Self::from_numerator(numerator.round() as u32)
    }

    pub(crate) fn numerator(self) -> u32 {
        self.numerator.get()
    }

    pub(crate) fn factor(self) -> f64 {
        f64::from(self.numerator()) / f64::from(FRACTIONAL_SCALE_DENOMINATOR)
    }
}

fn round_ratio_away_from_zero(value: i64, denominator: i64) -> i64 {
    let half = denominator / 2;
    if value >= 0 {
        (value + half) / denominator
    } else {
        -((-value + half) / denominator)
    }
}

/// Convert a surface-local size to the corresponding buffer size. The
/// fractional-scale protocol requires toplevel sizes to be rounded halfway
/// away from zero. Retaining the protocol numerator avoids floating-point
/// errors changing which direction a halfway value rounds.
pub(crate) fn surface_to_buffer_size(surface: i32, scale: WaylandScale) -> i32 {
    let scaled = i64::from(surface) * i64::from(scale.numerator());
    round_ratio_away_from_zero(scaled, i64::from(FRACTIONAL_SCALE_DENOMINATOR))
        .try_into()
        .expect("scaled Wayland surface size exceeds i32")
}

/// Convert a requested buffer size to a surface-local size. Round up so that
/// the resulting buffer cannot lose the last row or column of the terminal.
pub(crate) fn buffer_to_surface_size(buffer: i32, scale: WaylandScale) -> i32 {
    if buffer <= 0 {
        return 0;
    }

    let numerator = i64::from(scale.numerator());
    ((i64::from(buffer) * i64::from(FRACTIONAL_SCALE_DENOMINATOR) + numerator - 1) / numerator)
        .try_into()
        .expect("scaled Wayland buffer size exceeds i32")
}

/// Convert a buffer-local coordinate to its nearest surface-local coordinate.
/// Unlike sizes, coordinates use nearest rounding rather than ceiling.
pub(crate) fn buffer_to_surface_coordinate(buffer: i32, scale: WaylandScale) -> i32 {
    let scaled = i64::from(buffer) * i64::from(FRACTIONAL_SCALE_DENOMINATOR);
    round_ratio_away_from_zero(scaled, i64::from(scale.numerator()))
        .try_into()
        .expect("scaled Wayland buffer coordinate exceeds i32")
}

#[cfg(test)]
mod test {
    use super::{
        buffer_to_surface_coordinate, buffer_to_surface_size, surface_to_buffer_size, WaylandScale,
    };

    fn scale(numerator: u32) -> WaylandScale {
        WaylandScale::from_numerator(numerator).unwrap()
    }

    #[test]
    fn fractional_scale_uses_exact_protocol_rounding() {
        let cases = [
            (150, 802, 1003), // 125%: 1002.5 rounds up
            (180, 801, 1202), // 150%: 1201.5 rounds up
            (204, 5, 9),      // 170%: 8.5 rounds up
            (204, 15, 26),    // 170%: 25.5 rounds up
            (205, 12, 21),    // 170.833...%: 20.5 rounds up
        ];

        for (numerator, surface, expected) in cases {
            assert_eq!(
                surface_to_buffer_size(surface, scale(numerator)),
                expected,
                "numerator={numerator} surface={surface}"
            );
        }

        assert_eq!(surface_to_buffer_size(800, scale(150)), 1000);
        assert_eq!(surface_to_buffer_size(801, scale(150)), 1001);
    }

    #[test]
    fn requested_buffer_size_is_not_truncated() {
        assert_eq!(buffer_to_surface_size(1000, scale(150)), 800);
        assert_eq!(buffer_to_surface_size(1001, scale(150)), 801);
        assert_eq!(buffer_to_surface_size(1361, scale(204)), 801);
    }

    #[test]
    fn coordinates_round_to_nearest_surface_pixel() {
        assert_eq!(buffer_to_surface_coordinate(9, scale(204)), 5);
        assert_eq!(buffer_to_surface_coordinate(-9, scale(204)), -5);
    }

    #[test]
    fn scale_constructors_retain_the_protocol_numerator() {
        assert_eq!(WaylandScale::ONE.numerator(), 120);
        assert_eq!(WaylandScale::from_numerator(205).unwrap().numerator(), 205);
        assert_eq!(WaylandScale::from_integer(2).unwrap().numerator(), 240);
        let scale = WaylandScale::from_factor(1.70833).unwrap();
        assert_eq!(scale.numerator(), 205);
        assert!((scale.factor() - 205.0 / 120.0).abs() < f64::EPSILON);
        assert_eq!(WaylandScale::from_numerator(0), None);
    }
}
