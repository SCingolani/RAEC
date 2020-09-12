use core::fmt::Debug;
use core::iter::Sum;
use core::ops::{Add, Div, Mul, Sub};
use rand_distr::num_traits::Float;

pub const N_TAPS: usize = 1024;


pub struct NLMF<T> {
    pub weights: [T; N_TAPS],
    mu: T,
    eps: T,
}

impl<
        T: Float
            + Default
            + Debug
            + Sized
            + Copy
            + Clone
            + Sum
            + Add<Output = T>
            + Sub<Output = T>
            + Mul<Output = T>
            + Div<Output = T>,
    > NLMF<T>
{
    pub fn new(_n: usize, mu: T, eps: T, weights: [T; N_TAPS]) -> NLMF<T> {
        let initial_weights: [_; N_TAPS] = weights;
        NLMF {
            weights: initial_weights,
            mu: mu,
            eps: eps,
        }
    }

    pub fn adapt(&mut self, input: &[T], target: T, novelty_threshold: T) -> (T, T) {
        let output: T = self.weights.iter().zip(input).map(|(&w, &x)| w * x).sum();
        let error = target - output;
        let nu = self.mu / (self.eps + input.iter().zip(input).map(|(&x1, &x2)| x1 * x2).sum());
        //self.w += nu * x * e**3
        let mut novelty: T = T::default();
        for (&_w, &x) in self.weights.iter().zip(input) {
            let dw = nu * error * x;
            novelty = if (dw * error).abs() > novelty {
                (dw * error).abs()
            } else {
                novelty
            };
        }
        if novelty < novelty_threshold {
            for (w, &x) in self.weights.iter_mut().zip(input) {
                let dw = nu * error * x;
                *w = *w + dw;
                assert!(!(w.is_nan()));
            }
        };
        (output, novelty)
    }
}
