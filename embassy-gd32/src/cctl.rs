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
    pub pll: PLLConfig,
    pub ck_sys: ClockSrc,
    pub ahb_prediv: AHBPreDiv,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pll: PLLConfig::Off,
            ck_sys: ClockSrc::IRC8M,
        }
    }
}

pub(crate) fn init(rcu: &crate::pac::RCU, fmc: &crate::pac::FMC, config: &Config) {

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
    
    let (ck_sys_hz, scs_val) = match config.ck_sys {
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

    assert!(ck_sys_hz < Hertz::mhz(180));

    let ck_ahb = match.config.ahb_prediv {

    };

    //Set the flash wait state before changing the clock freq
    if ck_ahb <= Hertz::mhz(36) {
        fmc.ws.write(|w| unsafe { w.wscnt().bits(0) } );

    } else if ck_ahb <= Hertz::mhz(73) {
        fmc.ws.write(|w| unsafe { w.wscnt().bits(1) } );

    } else if ck_ahb <= Hertz::mhz(108) {
        fmc.ws.write(|w| unsafe { w.wscnt().bits(2) } );

    } else if ck_ahb <= Hertz::mhz(144) {
        fmc.ws.write(|w| unsafe { w.wscnt().bits(3) } );

    } else if ck_ahb <= Hertz::mhz(180) {
        fmc.ws.write(|w| unsafe { w.wscnt().bits(4) } );

    } else {
        panic!("invalid clock freq: {}", hz.0);
    }

    // set clock mux
    rcu.cfg0.write(|w| w.scs().variant(scs_val));



}