use core::fmt::Debug;
use core::iter::Sum;
use core::ops::{Add, Div, Mul, Sub};
use rand_distr::num_traits::Float;
use smallvec::{smallvec, SmallVec};

pub struct NLMF<T> {
    pub weights: SmallVec<[T; 1024]>,
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
    pub fn new(n: usize, mu: T, eps: T, weights: Vec<T>) -> NLMF<T> {
        let initial_weights: SmallVec<[_; 1024]> = SmallVec::from_vec(weights);
        NLMF {
            weights: initial_weights,
            mu: mu,
            eps: eps,
        }
    }

    pub fn adapt(&mut self, input: &[T], target: T) -> T {
        let output: T = self.weights.iter().zip(input).map(|(&w, &x)| w * x).sum();
        let error = target - output;
        let nu = self.mu / (self.eps + input.iter().zip(input).map(|(&x1, &x2)| x1 * x2).sum());
        //self.w += nu * x * e**3
        for (w, &x) in self.weights.iter_mut().zip(input) {
            *w = *w + nu * error * x;
            assert!(!(w.is_nan()));
        }
        output
    }
}
