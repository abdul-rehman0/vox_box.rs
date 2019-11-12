extern crate num;

use std::iter::*;
use std::ops::Neg;

use crate::error::*;

use num::{Float, FromPrimitive, One, Zero};
use num_complex::Complex;

pub trait Polynomial<'a, T> {
    fn degree(&self) -> usize;
    fn off_low(&self) -> usize;
    fn laguerre(&self, z: Complex<T>) -> Complex<T>;

    fn find_roots_work_size(&self) -> usize;
    fn find_roots(&self) -> VoxBoxResult<Vec<Complex<T>>>;
    fn find_roots_mut(&mut self, _: &mut [Complex<T>]) -> VoxBoxResult<()>;

    fn div_polynomial(&mut self, other: Complex<T>) -> VoxBoxResult<Vec<Complex<T>>>;
    fn div_polynomial_mut(
        &'a mut self,
        other: Complex<T>,
        rem: &'a mut [Complex<T>],
    ) -> VoxBoxResult<()>;
}

impl<'a, T> Polynomial<'a, T> for [Complex<T>]
where
    T: Float + FromPrimitive,
{
    fn degree(&self) -> usize {
        self.iter()
            .rposition(|r| r != &Complex::<T>::zero())
            .unwrap_or(0)
    }

    fn off_low(&self) -> usize {
        self.iter()
            .position(|r| r != &Complex::<T>::zero())
            .unwrap_or(0)
    }

    fn laguerre(&self, start: Complex<T>) -> Complex<T> {
        let n: usize = self.len() - 1;
        let mut z = start;
        // max iterations of 20
        for _ in 0..20 {
            let mut abg = [self[n], Complex::<T>::zero(), Complex::<T>::zero()];

            for j in (0..n).rev() {
                abg[2] = abg[2] * z + abg[1];
                abg[1] = abg[1] * z + abg[0];
                abg[0] = abg[0] * z + self[j];
            }

            if abg[0].norm() <= T::from(1.0e-16).unwrap() {
                return z;
            }

            let ca: Complex<T> = abg[1].neg() / abg[0];
            let ca2: Complex<T> = ca * ca;

            // H = 1/a^2 + (n-1)/b^2
            let cb: Complex<T> =
                ca2 - ((Complex::<T>::from(T::one() + T::one()) * abg[2]) / abg[0]);

            // sqrt((n-1)(nH-G^2))
            let c1: Complex<T> = ((Complex::<T>::from(T::from(n - 1).unwrap())
                * Complex::<T>::from(T::from(n).unwrap())
                * cb)
                - ca2)
                .sqrt();

            let cc1: Complex<T> = ca + c1;
            let cc2: Complex<T> = ca - c1;

            let cc = if cc1.norm() > cc2.norm() {
                Complex::<T>::from(T::from_usize(n).unwrap()) / cc1
            } else {
                Complex::<T>::from(T::from_usize(n).unwrap()) / cc2
            };

            z = z + cc;
        }
        z
    }

    /// Override to determine the necessary size of the Vec for the workspace
    fn find_roots_work_size(&self) -> usize {
        self.len() * 6 + 4
    }

    fn find_roots(&self) -> VoxBoxResult<Vec<Complex<T>>> {
        let mut work: Vec<Complex<T>> =
            vec![Complex::<T>::from(T::zero()); self.find_roots_work_size()];
        let mut other = self.to_vec();
        {
            other.find_roots_mut(&mut work[..])?;
        }
        while other[other.len() - 1] == Complex::<T>::zero() {
            other.pop();
        }
        Ok(other)
    }

    /// work must be 3*size+2 for complex floats (meaning 6*size+4 of the buffer)
    fn find_roots_mut<'b>(&'b mut self, work: &'b mut [Complex<T>]) -> VoxBoxResult<()> {
        // Initialize coefficient highs and lows
        let coeff_high = self.degree();
        if coeff_high < 1 {
            return Err(VoxBoxError::Polynomial(
                "Zero degree polynomial: no roots to be found.",
            ));
        }

        let coeff_low: usize = self.off_low();
        let mut m = coeff_high - coeff_low;

        // work should be 2*self.len()
        let (z_roots, work) = work.split_at_mut(2 * self.len());
        let mut z_root_index = 0;
        for item in z_roots.iter_mut().take(coeff_low) {
            *item = Complex::<T>::zero();
            z_root_index += 1;
        }

        let (mut rem, work) = work.split_at_mut(coeff_high - coeff_low + 1);
        let (coeffs, _) = work.split_at_mut(coeff_high - coeff_low + 1);

        coeffs[coeff_low..=coeff_high].clone_from_slice(&self[coeff_low..=coeff_high]);
        // println!("&[] coeffs: {:?}", coeffs);

        // Use the Laguerre method to factor out a single root
        for _ in (3..=m).rev() {
            let start = Complex::<T>::new(T::from(-2.0).unwrap(), T::from(-2.0).unwrap());
            let z = coeffs.laguerre(start);
            z_roots[z_root_index] = z;
            z_root_index += 1;
            // println!("z is {:?}", z);
            if coeffs.div_polynomial_mut(z.neg(), &mut rem).is_err() {
                return Err(VoxBoxError::Polynomial("Failed to find roots"));
            }
            // println!("&[] coeffs are: {:?}", coeffs);
            m -= 1;
        }

        // Solve quadradic equation
        if m == 2 {
            let a2 = coeffs[2] + coeffs[2];
            let d = ((coeffs[1] * coeffs[1])
                - (Complex::<T>::from(T::from_i8(4i8).unwrap()) * coeffs[2] * coeffs[0]))
                .sqrt();
            let x = coeffs[1].neg();
            // println!("a2: {:?}, d: {:?}, x: {:?}", a2, d, x);
            z_roots[z_root_index] = (x + d) / a2;
            z_roots[z_root_index + 1] = (x - d) / a2;
            z_root_index += 2;
        }
        // Solve linear equation
        if m == 1 {
            z_roots[z_root_index] = coeffs[0].neg() / coeffs[1];
            z_root_index += 1;
        }

        self[..=z_root_index].clone_from_slice(&z_roots[..=z_root_index]);

        for item in self.iter_mut().skip(z_root_index + 1) {
            *item = Complex::<T>::zero();
        }

        Ok(())
    }

    /// Divides self by other, and stores the remainder in rem
    fn div_polynomial_mut(
        &'a mut self,
        other: Complex<T>,
        rem: &'a mut [Complex<T>],
    ) -> VoxBoxResult<()> {
        rem[..self.len()].clone_from_slice(&self[..]);

        if other != Complex::<T>::zero() {
            let ns = self.degree();
            let ds = 1;
            for i in (0..=(ns - ds)).rev() {
                self[i] = rem[ds + i];
                for (j, item) in rem.iter_mut().enumerate().skip(i).take(ds) {
                    if j - i == 0 {
                        *item = *item - (self[i] * other);
                    } else if j - i == 1 {
                        *item = *item - (self[i] * Complex::<T>::one());
                    }
                }
            }
            // println!("self: {:?}", self);
            for _ in ds..=ns {
                rem[rem.degree()] = Complex::<T>::zero();
            }
            let l = self.degree();
            // println!("ns, ds: {:?}, {:?}, {:?}", ns, ds, l + 1);
            for _ in 0..=(l + 1) - ns - ds {
                self[self.degree()] = Complex::<T>::zero();
            }
            // println!("self: {:?}", &self);
            // println!("rem: {:?}", &rem);
            Ok(())
        } else if other != Complex::<T>::zero() {
            for f in rem.iter_mut() {
                *f = *f / other;
            }
            Ok(())
        } else {
            Err(VoxBoxError::Polynomial("Tried to divide by zero"))
        }
    }

    /// Returns the remainder
    fn div_polynomial(&mut self, other: Complex<T>) -> VoxBoxResult<Vec<Complex<T>>> {
        let mut rem = self.to_vec();
        {
            self.div_polynomial_mut(other, &mut rem[..])?;
        }
        Ok(rem)
    }
}

#[cfg(test)]
mod tests {
    extern crate num;

    pub use super::*;
    pub use num_complex::Complex;

    // Broken until I actually implement a POLYNOMIAL division
    // #[test]
    // fn test_div_polynomial() {
    //     let exp_quo: Vec<Complex<f64>> = vec![1.32, -0.8].iter().map(|v| Complex::<f64>::from(v)).collect();
    //     let exp_rem: Vec<Complex<f64>> = vec![-0.32].iter().map(|v| Complex::<f64>::from(v)).collect();
    //     let mut a: Vec<Complex<f64>> = vec![1f64, 2.5, -2.0].iter().map(|v| Complex::<f64>::from(v)).collect();
    //     let b = vec![1f64, 2.5].iter().map(|v| Complex::<f64>::from(v)).collect();
    //     {
    //         let rem = a.div_polynomial(b).unwrap();
    //         assert_eq!(rem.len(), exp_rem.len());
    //         for i in 0..exp_rem.len() {
    //             let diff: Complex<f64> = rem[i] - exp_rem[i];
    //             let re: f64 = diff.re;
    //             let im: f64 = diff.im;
    //             println!("diff: {:?}", diff);
    //             assert!(re.abs() < 1e-10);
    //             assert!(im.abs() < 1e-10);
    //         }
    //     }
    //     let eqlen: usize = exp_quo.len();
    //     assert_eq!(a.len(), eqlen);
    //     for i in 0..eqlen {
    //         let diff: Complex<f64> = a[i] - exp_quo[i];
    //         let re: f64 = diff.re;
    //         let im: f64 = diff.im;
    //         println!("diff: {:?}", diff);
    //         assert!(re.abs() < 1e-10);
    //         assert!(im.abs() < 1e-10);
    //     }
    // }
    //
    // #[test]
    // fn test_div_polynomial_f32() {
    //     let exp_quo: Vec<Complex<f32>> = vec![1.32f32, -0.8].iter().map(|v| Complex::<f32>::from(v)).collect();
    //     let exp_rem: Vec<Complex<f32>> = vec![-0.32f32].iter().map(|v| Complex::<f32>::from(v)).collect();
    //     let mut a: Vec<Complex<f32>> = vec![1f32, 2.5, -2.0].iter().map(|v| Complex::<f32>::from(v)).collect();
    //     let b: Vec<Complex<f32>> = vec![1f32, 2.5].iter().map(|v| Complex::<f32>::from(v)).collect();
    //     {
    //         let rem = a.div_polynomial(b).unwrap();
    //         println!("rem: {:?}", rem);
    //         assert_eq!(rem.len(), exp_rem.len());
    //         for i in 0..exp_rem.len() {
    //             let diff = rem[i] - exp_rem[i];
    //             assert!(diff.re.abs() < 1e-5);
    //             assert!(diff.im.abs() < 1e-5);
    //         }
    //     }
    //     assert_eq!(a.len(), exp_quo.len());
    //     for i in 0..exp_quo.len() {
    //         let diff = a[i] - exp_quo[i];
    //         assert!(diff.re.abs() < 1e-5);
    //         assert!(diff.im.abs() < 1e-5);
    //     }
    // }

    #[test]
    fn test_degree() {
        let a: Vec<Complex<f64>> = vec![3.0, 2.0, 4.0, 0.0, 0.0]
            .iter()
            .map(Complex::<f64>::from)
            .collect();
        assert_eq!(a.degree(), 2);
    }

    #[test]
    fn test_off_low() {
        let a: Vec<Complex<f64>> = vec![0.0f64, 0.0, 3.0, 2.0, 4.0]
            .iter()
            .map(Complex::<f64>::from)
            .collect();
        assert_eq!(a.off_low(), 2);
    }

    #[test]
    fn test_laguerre() {
        let vec: Vec<Complex<f64>> = vec![1.0, 2.5, 2.0, 3.0]
            .iter()
            .map(Complex::<f64>::from)
            .collect();
        let exp: Complex<f64> = Complex::<f64>::new(-0.1070229535872, -0.8514680262155);
        let point: Complex<f64> = Complex::<f64>::new(-64.0, -64.0);
        let res = vec.laguerre(point);
        let diff = exp - res;
        println!("res: {:?}", res);
        println!("diff: {:?}", diff);
        assert!(diff.re < 0.00000001);
        assert!(diff.im < 0.00000001);
    }

    #[test]
    fn test_1d_roots() {
        let poly: Vec<Complex<f64>> = vec![1.0, 2.5].iter().map(Complex::<f64>::from).collect();
        let roots = poly.find_roots().unwrap();
        let roots_exp = vec![Complex::<f64>::new(-0.4, 0.0)];
        assert_eq!(roots.len(), 1);
        for i in 0..roots_exp.len() {
            let diff = roots[i] - roots_exp[i];
            assert!(diff.re.abs() < 1e-12);
            assert!(diff.im.abs() < 1e-12);
        }
    }

    #[test]
    fn test_2d_roots() {
        let poly: Vec<Complex<f64>> = vec![1.0, 2.5, -2.0]
            .iter()
            .map(Complex::<f64>::from)
            .collect();
        let roots = poly.find_roots().unwrap();
        let roots_exp = vec![
            Complex::<f64>::new(-0.31872930440884, 0.0),
            Complex::<f64>::new(1.5687293044088, 0.0),
        ];
        println!("Roots found: {:?}", roots);
        assert_eq!(roots.len(), roots_exp.len());
        for i in 0..roots_exp.len() {
            let diff = roots[i] - roots_exp[i];
            assert!(diff.re.abs() < 1e-12);
            assert!(diff.im.abs() < 1e-12);
        }
    }

    #[test]
    fn test_2d_complex_roots() {
        let coeffs: Vec<Complex<f64>> = vec![1.0, -2.5, 2.0]
            .iter()
            .map(Complex::<f64>::from)
            .collect();
        let roots = coeffs.find_roots().unwrap();
        let roots_exp = vec![
            Complex::<f64>::new(0.625, -0.33071891388307),
            Complex::<f64>::new(0.625, 0.33071891388307),
        ];
        assert_eq!(roots.len(), roots_exp.len());
        println!("Roots found: {:?}", roots);
        for i in 0..roots_exp.len() {
            let diff = roots[i] - roots_exp[i];
            assert!(diff.re.abs() < 1e-12);
            assert!(diff.im.abs() < 1e-12);
        }
    }

    #[test]
    fn test_2d_complex_roots_f32() {
        let coeffs: Vec<Complex<f32>> = vec![1.0, -2.5, 2.0]
            .iter()
            .map(Complex::<f32>::from)
            .collect();
        let roots = coeffs.find_roots().unwrap();
        let roots_exp = vec![
            Complex::<f32>::new(0.625, -0.33071891388307),
            Complex::<f32>::new(0.625, 0.33071891388307),
        ];
        assert_eq!(roots.len(), roots_exp.len());
        println!("Roots found: {:?}", roots);
        for i in 0..roots_exp.len() {
            let diff = roots[i] - roots_exp[i];
            assert!(diff.re.abs() < 1e-12);
            assert!(diff.im.abs() < 1e-12);
        }
    }

    #[test]
    fn test_hi_d_roots() {
        let lpc_exp: Vec<Complex<f64>> = vec![1.0, 2.5, -2.0, -3.0]
            .iter()
            .map(Complex::<f64>::from)
            .collect();
        let roots_exp = vec![
            Complex::<f64>::new(-1.1409835232292, 0.0),
            Complex::<f64>::new(-0.35308705904629, 0.0),
            Complex::<f64>::new(0.82740391560878, 0.0),
        ];
        let roots = lpc_exp.find_roots().unwrap();
        println!("Roots: {:?}", roots);

        assert_eq!(roots.len(), roots_exp.len());
        for i in 0..roots_exp.len() {
            let diff = roots[i] - roots_exp[i];
            assert!(diff.re.abs() < 1e-6);
            assert!(diff.im.abs() < 1e-6);
        }
    }

    #[test]
    fn test_hi_d_roots_f32() {
        let lpc_exp: Vec<Complex<f32>> = vec![1.0, 2.5, -2.0, -3.0]
            .iter()
            .map(|v| Complex::<f32>::from(v))
            .collect();
        let roots_exp = vec![
            Complex::<f32>::new(-1.1409835232292, 0.0),
            Complex::<f32>::new(-0.35308705904629, 0.0),
            Complex::<f32>::new(0.82740391560878, 0.0),
        ];
        let roots = lpc_exp.find_roots().unwrap();
        println!("Roots: {:?}", roots);

        assert_eq!(roots.len(), roots_exp.len());
        for i in 0..roots_exp.len() {
            let diff = roots[i] - roots_exp[i];
            assert!(diff.re.abs() < 1e-6);
            assert!(diff.im.abs() < 1e-6);
        }
    }

    #[test]
    fn test_f32_roots() {
        let lpc_coeffs: Vec<Complex<f32>> = vec![
            1.0,
            -0.99640256,
            0.25383306,
            -0.25471634,
            0.5084799,
            -0.0685858,
            -0.35042483,
            0.07676613,
            -0.12874511,
            0.11829436,
            0.023972526,
        ]
        .iter()
        .map(Complex::<f32>::from)
        .collect();
        let roots = lpc_coeffs.laguerre(Complex::<f32>::new(-64.0, -64.0));
        println!("Roots: {:?}", roots);
        assert!(roots.re.is_finite());
        assert!(roots.im.is_finite());
    }
}
