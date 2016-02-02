extern crate num;
extern crate rustfft as fft;

use std::f64::consts::PI;
use std::ops::Index;
use num::{Complex, Float, ToPrimitive, FromPrimitive};
use num::traits::{Zero, Signed};
use super::waves::Filter;
use std::fmt::Debug;

const FFT_SIZE: usize = 512;

pub trait LPC<T> {
    fn lpc_mut(&self, n_coeffs: usize, ac: &mut [T], kc: &mut [T], tmp: &mut [T]);
    fn lpc(&self, n_coeffs: usize) -> Vec<T>;
}

impl<V: ?Sized, T> LPC<T> for V where 
    T: Float,
    V: Index<usize, Output=T>
{ 
    fn lpc_mut(&self, n_coeffs: usize, ac: &mut [T], kc: &mut [T], tmp: &mut [T]) {
        /* order 0 */
        let mut err = self[0];
        ac[0] = T::one();

        /* order >= 1 */
        for i in 1..n_coeffs+1 {
            let mut acc = self[i];
            for j in 1..i {
                acc = acc + (ac[j] * self[i-j]);
            }
            kc[i-1] = acc.neg() / err;
            ac[i] = kc[i-1];
            for j in 0..n_coeffs {
                tmp[j] = ac[j];
            }
            for j in 1..i {
                ac[j] = ac[j] + (kc[i-1] * tmp[i-j]);
            }
            err = err * (T::one() - (kc[i-1] * kc[i-1]));
        };
    }

    fn lpc(&self, n_coeffs: usize) -> Vec<T> {
        let mut ac: Vec<T> = vec![T::zero(); n_coeffs + 1];
        let mut kc: Vec<T> = vec![T::zero(); n_coeffs];
        let mut tmp: Vec<T> = vec![T::zero(); n_coeffs];
        self.lpc_mut(n_coeffs, &mut ac[..], &mut kc[..], &mut tmp[..]);
        ac
    }
}

#[derive(Clone, Debug)]
pub struct Resonance<T> {
    pub frequency: T,
    pub amplitude: T
}

impl<T: Float + FromPrimitive> Resonance<T> {
    pub fn from_root(root: &Complex<T>, sample_rate: T) -> Option<Resonance<T>> {
        let freq_mul: T = T::from_f64(sample_rate.to_f64().unwrap() / (PI * 2f64)).unwrap();
        if root.im >= T::zero() {
            let res = Resonance::<T> { 
                frequency: root.im.atan2(root.re) * freq_mul,
                amplitude: (root.im.powi(2) + root.re.powi(2)).sqrt()
            };
            if res.frequency > T::one() {
                Some(res)
            } else { None }
        } else { None }
    }
}

pub trait ToResonance<T> {
    fn to_resonance(&self, sample_rate: T) -> Vec<Resonance<T>>;
}

impl<T> ToResonance<T> for [Complex<T>] 
    where T: Float + 
             FromPrimitive 
{
    // Give it some roots, it'll find the resonances
    fn to_resonance(&self, sample_rate: T) -> Vec<Resonance<T>> {
        let mut res: Vec<Resonance<T>> = self.iter()
            .filter_map(|r| Resonance::<T>::from_root(r, sample_rate)).collect();
        res.sort_by(|a, b| (a.frequency.partial_cmp(&b.frequency)).unwrap());
        res
    }
}

pub struct FormantFrame<T: Float> {
    frequency: T,
}

pub struct FormantExtractor<'a, T: 'a + Float> {
    num_formants: usize,
    frame_index: usize,
    resonances: &'a Vec<Vec<T>>,
    pub estimates: Vec<T>
}

impl<'a, T: 'a + Float + PartialEq> FormantExtractor<'a, T> {
    pub fn new(
        num_formants: usize, 
        resonances: &'a Vec<Vec<T>>, 
        starting_estimates: Vec<T>) -> Self {
        FormantExtractor { 
            num_formants: num_formants, 
            resonances: resonances, 
            frame_index: 0,
            estimates: starting_estimates 
        }
    }
}

impl<'a, T: 'a + Float + PartialEq + FromPrimitive> Iterator for FormantExtractor<'a, T> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.resonances.len() == self.frame_index {
            return None;
        }

        let frame = self.resonances[self.frame_index].clone();
        let mut slots: Vec<Option<T>> = self.estimates.iter()
        .map(|estimate| {
            let mut indices: Vec<usize> = (0..frame.len()).collect();
            indices.sort_by(|a, b| {
                (frame[*a] - *estimate).abs().partial_cmp(&(frame[*b] - *estimate).abs()).unwrap()
            });
            let win = indices.first().unwrap().clone();
            Some(frame[win])
        }).collect();

        // Step 3: Remove duplicates. If the same peak p_j fills more than one slots S_i keep it
        // only in the slot S_k which corresponds to the estimate EST_k that it is closest to in
        // frequency, and remove it from any other slots.
        let mut w: usize = 0;
        let mut has_unassigned: bool = false;
        for r in 1..slots.len() {
            match slots[r] {
                Some(v) => { 
                    if v == slots[w].unwrap() {
                        if (v - self.estimates[r]).abs() < (v - self.estimates[w]).abs() {
                            slots[w] = None;
                            has_unassigned = true;
                            w = r;
                        } else {
                            slots[r] = None;
                            has_unassigned = true;
                        }
                    } else {
                        w = r;
                    }
                },
                None => { }
            }
        }

        if has_unassigned {
            // Step 4: Deal with unassigned peaks. If there are no unassigned peaks p_j, go to Step 5.
            // Otherwise, try to fill empty slots with peaks not assigned in Step 2 as follows.
            for j in 0..frame.len() {
                let peak = Some(frame[j]);
                if slots.contains(&peak) { continue; }
                match slots.clone().get(j) {
                    Some(&s) => {
                        match s {
                            Some(_) => { },
                            None => { slots[j] = peak; continue; }
                        }
                    }
                    None => { }
                }
                if j > 0 && j < slots.len() {
                    match slots.clone().get(j-1) {
                        Some(&s) => {
                            match s {
                                Some(_) => { },
                                None => { slots.swap(j, j-1); slots[j] = peak; continue; }
                            }
                        }
                        None => { }
                    }
                }
                match slots.clone().get(j+1) {
                    Some(&s) => {
                        match s {
                            Some(_) => { },
                            None => { slots.swap(j, j+1); slots[j] = peak; continue; }
                        }
                    }
                    None => { }
                }
            }
        }

        let mut winners: Vec<T> = slots.iter().filter_map(|v| *v).filter(|v| *v > T::zero()).collect();
        self.estimates = winners.clone();
        self.frame_index += 1;
        winners.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Some(winners)
    }
}

pub trait MFCC<T> {
    fn mfcc(&self, num_coeffs: usize, freq_bounds: (f64, f64), sample_rate: f64) -> Vec<T>;
}

pub fn hz_to_mel(hz: f64) -> f64 {
    1125. * (hz / 700.).ln_1p()
}

pub fn mel_to_hz(mel: f64) -> f64 {
    700. * ((mel / 1125.).exp() - 1.)
}

pub fn dct<T: FromPrimitive + ToPrimitive + Float>(signal: &[T]) -> Vec<T> {
    signal.iter().enumerate().map(|(k, val)| {
        T::from_f64(2. * (0..signal.len()).fold(0., |acc, n| {
            acc + signal[n].to_f64().unwrap() * (PI * k as f64 * (2. * n as f64 + 1.) / (2. * signal.len() as f64)).cos()
        })).unwrap()
    }).collect()
}

/// MFCC assumes that it is a windowed signal
impl<T: ?Sized> MFCC<T> for [T] 
    where T: Debug + 
             Float + 
             ToPrimitive + 
             FromPrimitive + 
             Into<Complex<T>> + 
             Zero + 
             Signed
{
    fn mfcc(&self, num_coeffs: usize, freq_bounds: (f64, f64), sample_rate: f64) -> Vec<T> {
        let mel_range = hz_to_mel(freq_bounds.1) - hz_to_mel(freq_bounds.0);
        // Still an iterator
        let points = (0..(num_coeffs + 2)).map(|i| (i as f64 / num_coeffs as f64) * mel_range + hz_to_mel(freq_bounds.0));
        let bins: Vec<usize> = points.map(|point| ((FFT_SIZE + 1) as f64 * mel_to_hz(point) / sample_rate).floor() as usize).collect();

        let mut spectrum = vec![Complex::<T>::from(T::zero()); FFT_SIZE];
        let mut fft = fft::FFT::new(FFT_SIZE, false);
        let signal: Vec<Complex<T>> = self.iter().map(|e| Complex::<T>::from(e)).collect();
        fft.process(&signal, &mut spectrum);

        let energies: Vec<T> = bins.windows(3).map(|window| {
            let up = window[1] - window[0];

            let up_sum = (window[0]..window[1]).enumerate().fold(0f64, |acc, (i, bin)| {
                let multiplier = i as f64 / up as f64;
                acc + spectrum[bin].norm_sqr().to_f64().unwrap().abs() * multiplier
            });

            let down = window[2] - window[1];
            let down_sum = (window[1]..window[2]).enumerate().fold(0f64, |acc, (i, bin)| {
                let multiplier = i as f64 / down as f64;
                acc + spectrum[bin].norm().to_f64().unwrap().abs() * multiplier
            });
            T::from_f64((up_sum + down_sum).log10()).unwrap_or(T::from_f32(1.0e-10).unwrap())
        }).collect();

        dct(&energies[..])
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::{thread_rng, Rng};
    use waves::*;

    #[test]
    fn test_hz_to_mel() {
        assert!(hz_to_mel(300.) - 401.25 < 1.0e-2);
    }

    #[test]
    fn test_mel_to_hz() {
        assert!(mel_to_hz(401.25) - 300. < 1.0e-2);
    }

    #[test]
    fn test_mfcc() {
        let mut rng = thread_rng();
        let mut vec: Vec<f64> = (0..super::FFT_SIZE).map(|_| rng.gen_range::<f64>(-1., 1.)).collect();
        vec.preemphasis(0.1f64 * 22_050.).window(WindowType::Hanning);
        let mfccs = vec.mfcc(26, (133., 6855.), 22_050.);
        println!("mfccs: {:?}", mfccs);
    }

    #[test]
    fn test_dct() {
        let signal = [0.2, 0.3, 0.4, 0.3];
        let dcts = dct(&signal[..]);
        let exp = [2.4, -0.26131, -0.28284, 0.10823];
        println!("dcts: {:?}", &dcts);
        for pair in dcts.iter().zip(exp.iter()) {
            assert!(pair.0 - pair.1 < 1.0e-5);
        }
    }
}
