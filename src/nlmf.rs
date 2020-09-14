use core::fmt::Debug;
use core::iter::Sum;
use core::ops::{Add, Div, Mul, Sub};
use rand_distr::num_traits::Float;

use packed_simd::f32x8;

pub const N_TAPS: usize = 1024;


pub struct NLMF<T> {
    pub weights: [T; N_TAPS],
    mu: T,
    eps: T,
}

impl NLMF<f32>
{
    pub fn new(_n: usize, mu: f32, eps: f32, weights: [f32; N_TAPS]) -> NLMF<f32> {
        let initial_weights: [_; N_TAPS] = weights;
        NLMF {
            weights: initial_weights,
            mu: mu,
            eps: eps,
        }
    }

    pub fn adapt(&mut self, input: &[f32], target: f32, novelty_threshold: f32) -> (f32, f32) {
        // let output: f32 = self.weights.iter().zip(input).map(|(&w, &x)| w * x).sum();
        let output: f32 = self.weights
            .chunks_exact(8)
            .map(f32x8::from_slice_unaligned)
            .zip(input.chunks_exact(8).map(f32x8::from_slice_unaligned))
            .map(|(a, b)| a * b)
            .sum::<f32x8>()
            .sum();

        let error: f32 = target - output;
        let input_dot = input
            .chunks_exact(8)
            .map(f32x8::from_slice_unaligned)
            .zip(input.chunks_exact(8).map(f32x8::from_slice_unaligned))
            .map(|(a, b)| a * b)
            .sum::<f32x8>()
            .sum();
        let nu: f32 = self.mu / (self.eps + input_dot);
        //self.w += nu * x * e**3
        let mut novelty: f32 = 0.0;
        let mut dws: [f32; N_TAPS] = [0.0; N_TAPS];
        for (i, x) in input.iter().enumerate() {
            let dw: f32 = nu * error * x;
            let nov = (dw * error).abs();
            if nov > novelty {
                novelty = nov;
            }
            dws[i] = dw;
        }
        if novelty < novelty_threshold {
            for (w, dw) in self.weights.iter_mut().zip(dws.iter()) {
                *w = *w + dw;
                assert!(!(w.is_nan()));
            }
        };
        (output, novelty)
    }
}
