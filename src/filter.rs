// Copied from oxcable library

//! A second order IIR filter.
//!
//! A `LowPass` or `HighPass` filter will provide a 3dB attenuation at the
//! cutoff frequency, and a 12dB per octave rolloff in the attenuation region.
//!
//! A `LowShelf` or `HighShelf` filter will provide a shelf starting at the
//! cutoff frequency, and will provide the specified gain in the shelf region.
//!
//! A `Peak` filter will provide the specified gain around a center frequency,
//! with the width of the peak determined by the Q. A higher Q means a narrower
//! peak.

use std::f32::consts::PI;

const SAMPLE_RATE: i32 = 48000;

fn decibel_to_ratio(db: f32) -> f32 {
    10.0_f32.powf(db / 10.0_f32)
}

/// Specifies the mode for a second order `Filter`.
///
/// Cutoffs are provided in Hz, gains are provided in decibels.
#[derive(Clone, Copy, Debug)]
pub enum FilterMode {
    /// LowPass(cutoff)
    LowPass(f32),
    /// HighPass(cutoff)
    HighPass(f32),
    /// LowShelf(cutoff, gain)
    LowShelf(f32, f32),
    /// HighShelf(cutoff, gain)
    HighShelf(f32, f32),
    /// Peak(cutoff, gain, Q)
    Peak(f32, f32, f32),
}
pub use self::FilterMode::*;

/// A two pole filter.
pub struct Filter {
    x_last1: f32,
    x_last2: f32, // two time step delay elements
    y_last1: f32,
    y_last2: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Filter {
    /// Creates a new second order filter with the provided mode. Each channel
    /// is filtered independently.
    pub fn new(mode: FilterMode) -> Self {
        // Compute the parameter values
        let (b0, b1, b2, a1, a2) = compute_parameters(mode);

        Filter {
            x_last1: 0.0_f32,
            x_last2: 0.0_f32,
            y_last1: 0.0_f32,
            y_last2: 0.0_f32,
            b0: b0,
            b1: b1,
            b2: b2,
            a1: a1,
            a2: a2,
        }
    }

    pub fn tick(&mut self, x: f32) -> f32 {
        // Run the all pass filter, and feedback the result
        let y = self.b0 * x + self.b1 * self.x_last1 + self.b2 * self.x_last2
            - self.a1 * self.y_last1
            - self.a2 * self.y_last2;

        // Store our results
        self.x_last2 = self.x_last1;
        self.y_last2 = self.y_last1;
        self.x_last1 = x;
        self.y_last1 = y;
        y
    }
}

/// Computes the parameters for our filter
#[allow(non_snake_case)]
fn compute_parameters(mode: FilterMode) -> (f32, f32, f32, f32, f32) {
    let cutoff = match mode {
        LowPass(cutoff) => cutoff,
        HighPass(cutoff) => cutoff,
        LowShelf(cutoff, _) => cutoff,
        HighShelf(cutoff, _) => cutoff,
        Peak(center, _, _) => center,
    };
    let K = (PI * cutoff / (SAMPLE_RATE as f32)).tan();

    match mode {
        LowPass(_) => {
            let b0 = K * K / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
            let b1 = 2.0_f32 * K * K / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
            let b2 = K * K / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
            let a1 = 2.0_f32 * (K * K - 1.0_f32) / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
            let a2 =
                (1.0_f32 - 2.0_f32.sqrt() * K + K * K) / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
            (b0, b1, b2, a1, a2)
        }
        HighPass(_) => {
            let b0 = 1.0_f32 / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
            let b1 = -2.0_f32 / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
            let b2 = 1.0_f32 / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
            let a1 = 2.0_f32 * (K * K - 1.0_f32) / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
            let a2 =
                (1.0_f32 - 2.0_f32.sqrt() * K + K * K) / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
            (b0, b1, b2, a1, a2)
        }
        LowShelf(_, gain) => {
            if gain < 0.0_f32 {
                // cut
                let V0 = 1.0_f32 / decibel_to_ratio(gain / 2.0_f32); // amplitude dB
                let b0 = (1.0_f32 + 2.0_f32.sqrt() * K + K * K)
                    / (1.0_f32 + (2.0_f32 * V0).sqrt() * K + V0 * K * K);
                let b1 = 2.0_f32 * (K * K - 1.0_f32)
                    / (1.0_f32 + (2.0_f32 * V0).sqrt() * K + V0 * K * K);
                let b2 = (1.0_f32 - 2.0_f32.sqrt() * K + K * K)
                    / (1.0_f32 + (2.0_f32 * V0).sqrt() * K + V0 * K * K);
                let a1 = 2.0_f32 * (V0 * K * K - 1.0_f32)
                    / (1.0_f32 + (2.0_f32 * V0).sqrt() * K + V0 * K * K);
                let a2 = (1.0_f32 - (2.0_f32 * V0).sqrt() * K + V0 * K * K)
                    / (1.0_f32 + (2.0_f32 * V0).sqrt() * K + V0 * K * K);
                (b0, b1, b2, a1, a2)
            } else {
                // boost
                let V0 = decibel_to_ratio(gain / 2.0_f32); // amplitude dB
                let b0 = (1.0_f32 + (2.0_f32 * V0).sqrt() * K + V0 * K * K)
                    / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
                let b1 = 2.0_f32 * (V0 * K * K - 1.0_f32) / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
                let b2 = (1.0_f32 - (2.0_f32 * V0).sqrt() * K + V0 * K * K)
                    / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
                let a1 = 2.0_f32 * (K * K - 1.0_f32) / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
                let a2 =
                    (1.0_f32 - 2.0_f32.sqrt() * K + K * K) / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
                (b0, b1, b2, a1, a2)
            }
        }
        HighShelf(_, gain) => {
            if gain < 0.0_f32 {
                // cut
                let V0 = 1.0_f32 / decibel_to_ratio(gain / 2.0_f32); // amplitude dB
                let b0 = (1.0_f32 + 2.0_f32.sqrt() * K + K * K)
                    / (V0 + (2.0_f32 * V0).sqrt() * K + K * K);
                let b1 = 2.0_f32 * (K * K - 1.0_f32) / (V0 + (2.0_f32 * V0).sqrt() * K + K * K);
                let b2 = (1.0_f32 - 2.0_f32.sqrt() * K + K * K)
                    / (V0 + (2.0_f32 * V0).sqrt() * K + K * K);
                let a1 = 2.0_f32 * (K * K / V0 - 1.0_f32)
                    / (1.0_f32 + (2.0_f32 / V0).sqrt() * K + K * K / V0);
                let a2 = (1.0_f32 - (2.0_f32 / V0).sqrt() * K + K * K / V0)
                    / (1.0_f32 + (2.0_f32 / V0).sqrt() * K + K * K / V0);
                (b0, b1, b2, a1, a2)
            } else {
                // boost
                let V0 = decibel_to_ratio(gain / 2.0_f32); // amplitude dB
                let b0 = (V0 + (2.0_f32 * V0).sqrt() * K + K * K)
                    / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
                let b1 = 2.0_f32 * (K * K - V0) / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
                let b2 = (V0 - (2.0_f32 * V0).sqrt() * K + K * K)
                    / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
                let a1 = 2.0_f32 * (K * K - 1.0_f32) / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
                let a2 =
                    (1.0_f32 - 2.0_f32.sqrt() * K + K * K) / (1.0_f32 + 2.0_f32.sqrt() * K + K * K);
                (b0, b1, b2, a1, a2)
            }
        }
        Peak(_, gain, Q) => {
            if gain < 0.0_f32 {
                // cut
                let V0 = 1.0_f32 / decibel_to_ratio(gain / 2.0_f32); // amplitude dB
                let b0 = (1.0_f32 + K / Q + K * K) / (1.0_f32 + V0 * K / Q + K * K);
                let b1 = 2.0_f32 * (K * K - 1.0_f32) / (1.0_f32 + V0 * K / Q + K * K);
                let b2 = (1.0_f32 - K / Q + K * K) / (1.0_f32 + V0 * K / Q + K * K);
                let a1 = 2.0_f32 * (K * K - 1.0_f32) / (1.0_f32 + V0 * K / Q + K * K);
                let a2 = (1.0_f32 - V0 * K / Q + K * K) / (1.0_f32 + V0 * K / Q + K * K);
                (b0, b1, b2, a1, a2)
            } else {
                // boost
                let V0 = decibel_to_ratio(gain / 2.0_f32); // amplitude dB
                let b0 = (1.0_f32 + V0 * K / Q + K * K) / (1.0_f32 + K / Q + K * K);
                let b1 = 2.0_f32 * (K * K - 1.0_f32) / (1.0_f32 + K / Q + K * K);
                let b2 = (1.0_f32 - V0 * K / Q + K * K) / (1.0_f32 + K / Q + K * K);
                let a1 = 2.0_f32 * (K * K - 1.0_f32) / (1.0_f32 + K / Q + K * K);
                let a2 = (1.0_f32 - K / Q + K * K) / (1.0_f32 + K / Q + K * K);
                (b0, b1, b2, a1, a2)
            }
        }
    }
}
