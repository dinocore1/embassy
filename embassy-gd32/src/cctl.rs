use crate::time::{ClockDivider, ClockMultiplier};
use crate::time::Hertz;

#[derive(Debug, Clone, Copy)]
pub enum ClockSrc {
    IRC8M,
    PLL,
    HXTAL(Hertz),
}

#[derive(Debug, Clone, Copy)]
pub enum PLLSource {
    IRC8MDiv2,
    HXTAL(Hertz, PLLPreDiv),
    IRC48M(PLLPreDiv),
}

#[derive(Debug, Clone, Copy)]
pub enum PLLConfig {
    Off,
    On(PLLSource, PLLMul),
}

#[derive(Debug, Clone, Copy)]
pub struct PLLMul(u8);

impl PLLMul {
    pub fn factor(mf: u8) -> Self {
        assert!(mf > 1 && mf < 64);
        Self(mf)
    }

    fn get_bits(&self) -> u8 {
        self.0 - 2
    }
}

impl ClockMultiplier for PLLMul {
    fn multiply(&self, hz: Hertz) -> Hertz {
        Hertz::hz(hz.0 * self.0 as u32)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PLLPreDiv {
    Div1,
    Div2,
}

impl ClockDivider for PLLPreDiv {
    fn divide(&self, hz: Hertz) -> Hertz {
        match self {
            PLLPreDiv::Div1 => hz,
            PLLPreDiv::Div2 => Hertz::hz(hz.0 / 2),
        }
    }
}

pub struct Config {
    pll: PLLConfig,
    ck_sys: ClockSrc,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pll: PLLConfig::Off,
            ck_sys: ClockSrc::IRC8M,
        }
    }
}

pub(crate) fn init(rcu: &crate::pac::RCU, config: &Config) {

    let pll_hz = match config.pll {
        PLLConfig::Off => {
            //disable the PLL
            rcu.ctl.write(|w| w.pllen().clear_bit());
            Hertz::hz(0)
        },
        PLLConfig::On(src, mul) => {

            // set the pll source
            let pll_hz = match src {
                PLLSource::IRC8MDiv2 => {
                    rcu.cfg0.write(|w| w.pllsel().clear_bit());
                    Hertz::mhz(4) * mul
                },
                PLLSource::HXTAL(hz, prediv) => {
                    rcu.cfg0.write(|w| w.pllsel().set_bit());
                    hz / prediv * mul
                },
                PLLSource::IRC48M(prediv) => {
                    rcu.cfg0.write(|w| w.pllsel().set_bit());
                    Hertz::mhz(48) / prediv * mul
                },
            };

            //set the multiplication factor
            rcu.cfg0.write(|w| {
                let bits = mul.get_bits();
                let w = w.pllmf_5().variant(0b10000 & bits != 0);
                let w = w.pllmf_4().variant(0b01000 & bits != 0);
                w.pllmf_3_0().variant(0b001111 & bits)
            });

            //enable the PLL
            rcu.ctl.write(|w| w.pllen().set_bit());

            pll_hz
        },
    };
    
    let (hz, scs_val) = match config.ck_sys {
        ClockSrc::IRC8M => {
            (Hertz::mhz(8), 0b00)
        },
        ClockSrc::HXTAL(hz) => {
            rcu.ctl.write(|w| w.hxtalen().set_bit());
            (hz, 0b01)
        },
        ClockSrc::PLL => {
            (pll_hz, 0b10)
        },
    };

    rcu.cfg0.write(|w| w.scs().variant(scs_val));



}