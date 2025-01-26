// Jackson Coxson

use crate::{Idevice, IdeviceError};

pub struct CoreDeviceProxy {
    pub idevice: Idevice,
}

impl CoreDeviceProxy {
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }
}
