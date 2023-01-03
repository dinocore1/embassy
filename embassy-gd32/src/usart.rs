
use crate::utils::{ClockDivider, ClockMultiplier, Hertz};

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Oversample {
    EightTimes,
    SixteenTimes,
}

fn calc_bauddiv(pclk: Hertz, baud: u32, oversample: Oversample) -> u16 {

    let (intdiv, fradiv) = match oversample {
        Oversample::SixteenTimes => {
            let div = (pclk.0 + baud/2) / baud;
            let intdiv = div & 0xfff0;
            let fradiv = div & 0xf;

            (intdiv, fradiv)
        }

        Oversample::EightTimes => {
            let div = ((pclk.0 + baud/2) << 1) / baud;
            let intdiv = div & 0xfff0;
            let fradiv = div & 0xf;

            (intdiv, fradiv)
        }
    };

    (intdiv as u16) | (fradiv as u16)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_calc_bauddiv_16() {
        let pclk = Hertz::mhz(32);
        let baud = 115200;
        let bauddiv = calc_bauddiv(pclk, baud, Oversample::SixteenTimes);

        //17.36
        let intdiv = bauddiv >> 4;
        assert_eq!(17, intdiv);

        let fradiv = bauddiv & 0xf;
        assert_eq!(6, fradiv);
    }

    #[test]
    fn test_calc_bauddiv_8() {
        let pclk = Hertz::mhz(32);
        let baud = 115200;
        let bauddiv = calc_bauddiv(pclk, baud, Oversample::EightTimes);

        //34.72
        let intdiv = bauddiv >> 4;
        assert_eq!(34, intdiv);

        let fradiv = bauddiv & 0xf;
        assert_eq!(12, fradiv);
    }

    #[test]
    fn test2_calc_bauddiv_16() {
        let pclk = Hertz::mhz(32);
        let baud = 900;
        let bauddiv = calc_bauddiv(pclk, baud, Oversample::SixteenTimes);

        //2222.25
        let intdiv = bauddiv >> 4;
        assert_eq!(2222, intdiv);

        let fradiv = bauddiv & 0xf;
        assert_eq!(4, fradiv);
    }
}