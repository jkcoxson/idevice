// Jackson Coxson

use std::collections::HashMap;

use crate::{IdeviceError, ReadWrite};

pub struct RemoteServerClient {
    idevice: Box<dyn ReadWrite>,
    current_message: usize,
    last_channel: usize,
    channels: HashMap<u8, Vec<super::message_aux::MessageAux>>,
}

impl RemoteServerClient {
    pub fn new(idevice: Box<dyn ReadWrite>) -> Result<Self, IdeviceError> {
        Ok(Self {
            idevice,
            current_message: 0,
            last_channel: 0,
            channels: HashMap::new(),
        })
    }
}

pub struct Channel {}
