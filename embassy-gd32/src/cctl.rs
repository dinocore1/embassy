use core::mem::MaybeUninit;

use crate::utils::{Hertz, ClockDivider, ClockMultiplier};

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

#[derive(Debug, Clone, Copy)]
pub enum AHBPreDiv {
    None,
    Div2,
    Div4,
    Div8,
    Div16,
    Div64,
    Div128,
    Div256,
    Div512,
}

impl AHBPreDiv {
    fn operate(&self, hz: Hertz) -> (Hertz, u8) {
        match self {
            AHBPreDiv::None => (hz, 0b0000),
            AHBPreDiv::Div2 => (Hertz::hz(hz.0 / 2), 0b1000),
            AHBPreDiv::Div4 => (Hertz::hz(hz.0 / 4), 0b1001),
            AHBPreDiv::Div8 => (Hertz::hz(hz.0 / 8), 0b1010),
            AHBPreDiv::Div16 => (Hertz::hz(hz.0 / 16), 0b1011),
            AHBPreDiv::Div64 => (Hertz::hz(hz.0 / 64), 0b1100),
            AHBPreDiv::Div128 => (Hertz::hz(hz.0 / 128), 0b1101),
            AHBPreDiv::Div256 => (Hertz::hz(hz.0 / 256), 0b1110),
            AHBPreDiv::Div512 => (Hertz::hz(hz.0 / 512), 0b1111),
        }
    }
}

impl ClockDivider for AHBPreDiv {
    fn divide(&self, hz: Hertz) -> Hertz {
        let (hz, bits) = self.operate(hz);
        hz
    }
}

#[derive(Debug, Clone, Copy)]
pub enum APBPreDiv {
    None,
    Div2,
    Div4,
    Div8,
    Div16,
}

impl APBPreDiv {
    fn operate(&self, hz: Hertz) -> (Hertz, u8) {
        match self {
            APBPreDiv::None => (hz, 0b000),
            APBPreDiv::Div2 => (Hertz::hz(hz.0 / 2), 0b100),
            APBPreDiv::Div4 => (Hertz::hz(hz.0 / 4), 0b101),
            APBPreDiv::Div8 => (Hertz::hz(hz.0 / 8), 0b110),
            APBPreDiv::Div16 => (Hertz::hz(hz.0 / 16), 0b111),
        }
    }
}

impl ClockDivider for APBPreDiv {
    fn divide(&self, hz: Hertz) -> Hertz {
        let (hz, bits) = self.operate(hz);
        hz
    }
}

pub enum LXTALConfig {
    None,
    Enable(Hertz),
}

pub struct Config {
    pub pll: PLLConfig,
    pub ck_sys: ClockSrc,
    pub ahb_prediv: AHBPreDiv,
    pub apb1_prediv: APBPreDiv,
    pub apb2_prediv: APBPreDiv,
    pub lxtal: LXTALConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pll: PLLConfig::Off,
            ck_sys: ClockSrc::IRC8M,
            ahb_prediv: AHBPreDiv::None,
            apb1_prediv: APBPreDiv::None,
            apb2_prediv: APBPreDiv::None,
            lxtal: LXTALConfig::None,
        }
    }
}

mod sealed {

}

pub trait CCTLPeripherial {
    fn frequency() -> crate::utils::Hertz;
    fn enable();
    fn disable();
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Clocks {
    pub sys: Hertz,
    pub ahb: Hertz,
    pub apb1: Hertz,
    pub apb2: Hertz,
    pub rtc: Hertz,
}

static mut CLOCK_FREQS: MaybeUninit<Clocks> = MaybeUninit::uninit();

pub(crate) fn get_freq() -> &'static Clocks {
    unsafe { &*CLOCK_FREQS.as_ptr() }
}


pub(crate) fn init(rcu: &crate::pac::RCU, fmc: &crate::pac::FMC, config: &Config) {

    let pll_hz = match config.pll {
        PLLConfig::Off => {
            //disable the PLL
            rcu.ctl.modify(|_, w| w.pllen().clear_bit());
            Hertz::hz(0)
        },
        PLLConfig::On(src, mul) => {

            // set the pll source
            let pll_hz = match src {
                PLLSource::IRC8MDiv2 => {
                    rcu.cfg0.modify(|_, w| w.pllsel().clear_bit());
                    Hertz::mhz(4) * mul
                },
                PLLSource::HXTAL(hz, prediv) => {
                    rcu.cfg0.modify(|_, w| w.pllsel().set_bit());
                    hz / prediv * mul
                },
                PLLSource::IRC48M(prediv) => {
                    rcu.cfg0.modify(|_, w| w.pllsel().set_bit());
                    Hertz::mhz(48) / prediv * mul
                },
            };

            //set the multiplication factor
            rcu.cfg0.modify(|_, w| {
                let bits = mul.get_bits();
                let w = w.pllmf_5().variant(0b10000 & bits != 0);
                let w = w.pllmf_4().variant(0b01000 & bits != 0);
                w.pllmf_3_0().variant(0b001111 & bits)
            });

            //enable the PLL
            rcu.ctl.modify(|_, w| w.pllen().set_bit());

            pll_hz
        },
    };
    
    let (ck_sys_hz, scs_val) = match config.ck_sys {
        ClockSrc::IRC8M => {
            (Hertz::mhz(8), 0b00)
        },
        ClockSrc::HXTAL(hz) => {
            rcu.ctl.modify(|_, w| w.hxtalen().set_bit());
            (hz, 0b01)
        },
        ClockSrc::PLL => {
            (pll_hz, 0b10)
        },
    };

    assert!(ck_sys_hz <= Hertz::mhz(180));

    let (ck_ahb, ahb_psc_bits) = config.ahb_prediv.operate(ck_sys_hz);

    let (ck_apb1, apb1_psc_bits) = config.apb1_prediv.operate(ck_ahb);
    assert!(ck_apb1 <= Hertz::mhz(90));

    let (ck_apb2, apb2_psc_bits) = config.apb2_prediv.operate(ck_ahb);

    //write the bus prescaler factors
    rcu.cfg0.modify(|_, w| w
            .ahbpsc().variant(ahb_psc_bits)
            .apb1psc().variant(apb1_psc_bits)
            .apb2psc().variant(apb2_psc_bits)
        );

    let clocks = Clocks {
        sys: ck_sys_hz,
        ahb: ck_ahb,
        apb1: ck_apb1,
        apb2: ck_apb2,
        rtc: Hertz(0),
    };

    //Set the flash wait state before changing the clock freq
    if ck_ahb <= Hertz::mhz(36) {
        fmc.ws.modify(|_, w| unsafe { w.wscnt().bits(0) } );

    } else if ck_ahb <= Hertz::mhz(73) {
        fmc.ws.modify(|_, w| unsafe { w.wscnt().bits(1) } );

    } else if ck_ahb <= Hertz::mhz(108) {
        fmc.ws.modify(|_, w| unsafe { w.wscnt().bits(2) } );

    } else if ck_ahb <= Hertz::mhz(144) {
        fmc.ws.modify(|_, w| unsafe { w.wscnt().bits(3) } );

    } else if ck_ahb <= Hertz::mhz(180) {
        fmc.ws.modify(|_, w| unsafe { w.wscnt().bits(4) } );

    } else {
        panic!("invalid clock freq: {}", ck_ahb.0);
    }

    // set clock mux
    rcu.cfg0.modify(|_, w| w.scs().variant(scs_val));

    info!("Clock freq: {}", clocks);

    unsafe { CLOCK_FREQS.write(clocks); }



}