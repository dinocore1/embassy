use core::mem::MaybeUninit;

use atomic_polyfill::{compiler_fence, Ordering};

use crate::utils::{ClockDivider, ClockMultiplier, Hertz};

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
        let (hz, _bits) = self.operate(hz);
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
        let (hz, _bits) = self.operate(hz);
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

mod sealed {}

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

#[inline]
fn enter_high_driver_mode() {
    let pmu = unsafe { &*crate::pac::PMU::ptr() };
    let rcu = unsafe { &*crate::pac::RCU::ptr() };

    // enable the PMU clock
    rcu.apb1en.modify(|_, w| w.pmuen().set_bit());

    // enable 1.1v high-drive mode for high-frequency CPU
    pmu.ctl0.modify(|_, w| w.hden().set_bit());

    // wait for the high-drive ready flag
    while pmu.cs0.read().hdrf().bit_is_clear() {}

    // set the high-drive switch
    pmu.ctl0.modify(|_, w| w.hds().set_bit());

    // wait for the high-drive switch
    while pmu.cs0.read().hdsrf().bit_is_clear() {}
}

pub(crate) fn init(rcu: &crate::pac::RCU, fmc: &crate::pac::FMC, config: &Config) {
    let pll_hz = match config.pll {
        PLLConfig::Off => {
            //disable the PLL
            rcu.ctl.modify(|_, w| w.pllen().clear_bit());
            Hertz::hz(0)
        }
        PLLConfig::On(src, mul) => {
            // set the pll source
            let (pll_hz, pllsel) = match src {
                PLLSource::IRC8MDiv2 => {
                    //wait for IRC8M is stable
                    while rcu.ctl.read().irc8mstb().bit_is_clear() {}
                    (Hertz::mhz(4) * mul, false)
                }
                PLLSource::HXTAL(hz, prediv) => {
                    //wait for HXTAL is stable
                    while rcu.ctl.read().hxtalstb().bit_is_clear() {}
                    (hz / prediv * mul, true)
                }
                PLLSource::IRC48M(prediv) => (Hertz::mhz(48) / prediv * mul, true),
            };

            //set the multiplication factor
            rcu.cfg0.modify(|r, w| {
                let mf = match mul.0 {
                    2..=16 => mul.0 - 2,
                    17..=64 => mul.0 - 1,
                    _ => unreachable!(),
                } as u32;
                //let w = w.pllmf_3_0().variant(0b00_1111 & mf);
                //let w = w.pllmf_4().variant(0b01_0000 & mf != 0);
                // pllmf_5 is WRONG this should be bit 30, instead, its 29. Someone did a typo
                //let w = w.pllmf_5().variant(0b10_0000 & mf != 0);

                let mut bits = r.bits();
                bits &= !(0x0F << 18);
                bits |= (0x0F & mf) << 18;

                if (0b01_0000 & mf) != 0 {
                    bits |= 1 << 27;
                }

                if (0b10_0000 & mf) != 0 {
                    bits |= 1 << 30;
                }

                let w = unsafe { w.bits(bits) };

                let w = match pllsel {
                    true => w.pllsel().set_bit(),
                    false => w.pllsel().clear_bit(),
                };

                w
            });

            //enable the PLL
            rcu.ctl.modify(|_, w| w.pllen().set_bit());

            //wait for the PLL to become stable
            while rcu.ctl.read().pllstb().bit_is_clear() {}

            pll_hz
        }
    };

    let (ck_sys_hz, scs_val) = match config.ck_sys {
        ClockSrc::IRC8M => (Hertz::mhz(8), 0b00),
        ClockSrc::HXTAL(hz) => {
            rcu.ctl.modify(|_, w| w.hxtalen().set_bit());
            (hz, 0b01)
        }
        ClockSrc::PLL => (pll_hz, 0b10),
    };

    assert!(ck_sys_hz <= Hertz::mhz(180));

    let (ck_ahb, ahb_psc_bits) = config.ahb_prediv.operate(ck_sys_hz);

    let (ck_apb1, apb1_psc_bits) = config.apb1_prediv.operate(ck_ahb);
    assert!(ck_apb1 <= Hertz::mhz(90));

    let (ck_apb2, apb2_psc_bits) = config.apb2_prediv.operate(ck_ahb);

    if ck_ahb >= Hertz::mhz(70) {
        enter_high_driver_mode();
    }

    //write the bus prescaler factors
    rcu.cfg0.modify(|_, w| {
        w.ahbpsc()
            .variant(ahb_psc_bits)
            .apb1psc()
            .variant(apb1_psc_bits)
            .apb2psc()
            .variant(apb2_psc_bits)
    });

    let clocks = Clocks {
        sys: ck_sys_hz,
        ahb: ck_ahb,
        apb1: ck_apb1,
        apb2: ck_apb2,
        rtc: Hertz(0),
    };

    //Set the flash wait state before changing the clock freq
    if ck_ahb <= Hertz::mhz(36) {
        fmc.ws.modify(|_, w| unsafe { w.wscnt().bits(0) });
    } else if ck_ahb <= Hertz::mhz(73) {
        fmc.ws.modify(|_, w| unsafe { w.wscnt().bits(1) });
    } else if ck_ahb <= Hertz::mhz(108) {
        fmc.ws.modify(|_, w| unsafe { w.wscnt().bits(2) });
    } else if ck_ahb <= Hertz::mhz(144) {
        fmc.ws.modify(|_, w| unsafe { w.wscnt().bits(3) });
    } else if ck_ahb <= Hertz::mhz(180) {
        fmc.ws.modify(|_, w| unsafe { w.wscnt().bits(4) });
    } else {
        panic!("invalid clock freq: {}", ck_ahb);
    }

    // set clock mux
    rcu.cfg0.modify(|_, w| w.scs().variant(scs_val));

    // wait for the clock mux to change
    while rcu.cfg0.read().scss().bits() != scs_val {}

    info!("Clock freq: {}", clocks);

    unsafe {
        CLOCK_FREQS.write(clocks);
    }
}
