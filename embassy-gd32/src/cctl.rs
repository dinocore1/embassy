pub use crate::time::Hertz;

#[derive(Debug, Clone, Copy)]
pub enum ClockSrc {
    IRC8M,
    PLL(PLLSource, PLLMul),
    HXTAL(Hertz),
}

#[derive(Debug, Clone, Copy)]
pub enum PLLSource {
    IRC8MDiv2,
    HXTAL(Hertz, PLLPreDiv),
    IRC48M(PLLPreDiv),
}

#[derive(Debug, Clone, Copy)]
pub struct PLLMul(u8);


#[derive(Debug, Clone, Copy)]
pub enum PLLPreDiv {
    Div1,
    Div2,
}

pub struct Config {
    ck_sys: ClockSrc,
}

impl Default for Config {
    fn default() -> Self {
        Self { 
            ck_sys: ClockSrc::IRC8M,
        }
    }
}

pub(crate) fn init(rcu: &crate::pac::RCU, config: &Config) {
    
    match config.ck_sys {
        ClockSrc::IRC8M => todo!(),
        ClockSrc::PLL(hz, pre_div) => todo!(),
        ClockSrc::HXTAL(hz) => {
            rcu.ctl.write(|w| w.hxtalen().set_bit() );
            


        },
    }



}