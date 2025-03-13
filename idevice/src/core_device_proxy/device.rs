// Jackson Coxson

use log::{debug, warn};
use pcap_file::pcapng::PcapNgWriter;
use smoltcp::{
    phy::{self, Device, DeviceCapabilities, Medium},
    time::Instant,
};
use tokio::sync::{
    mpsc::{error::TryRecvError, unbounded_channel, UnboundedReceiver, UnboundedSender},
    oneshot::{channel, Sender},
};

use crate::IdeviceError;

use super::CoreDeviceProxy;

pub struct ProxyDevice {
    sender: UnboundedSender<(Vec<u8>, Sender<Option<IdeviceError>>)>,
    receiver: UnboundedReceiver<Result<Vec<u8>, IdeviceError>>,
    mtu: usize,
}

impl ProxyDevice {
    pub fn new(mut proxy: CoreDeviceProxy) -> Self {
        let (sender, mut stack_recv) = unbounded_channel();
        let (stack_send, receiver) = unbounded_channel();
        let mtu = proxy.mtu as usize;

        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    res = proxy.recv() => {
                        // debug!("stack recv: {res:02X?}");
                        if stack_send.send(res).is_err() {
                            warn!("Interface failed to recv");
                            break;
                        }
                    }
                    pkt = stack_recv.recv() => {
                        let pkt: (Vec<u8>, Sender<Option<IdeviceError>>) = match pkt {
                            Some(p) => p,
                            None => {
                                warn!("Interface sender closed");
                                break;
                            }
                        };

                        // debug!("stack send: {:02X?}", pkt.0);
                        pkt.1.send(proxy.send(&pkt.0).await.err()).ok();

                    }
                }
            }
        });

        Self {
            sender,
            receiver,
            mtu,
        }
    }
}

impl Device for ProxyDevice {
    type RxToken<'a> = RxToken;
    type TxToken<'a> = TxToken;

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = self.mtu;
        caps
    }

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        match self.receiver.try_recv() {
            Ok(Ok(buffer)) => {
                let rx = RxToken { buffer };
                let tx = TxToken {
                    sender: self.sender.clone(),
                };
                Some((rx, tx))
            }
            Ok(Err(e)) => {
                warn!("Failed to recv message: {e:?}");
                None
            }
            Err(TryRecvError::Disconnected) => {
                warn!("Proxy sender is closed");
                None
            }
            Err(TryRecvError::Empty) => None,
        }
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(TxToken {
            sender: self.sender.clone(),
        })
    }
}

#[doc(hidden)]
pub struct RxToken {
    buffer: Vec<u8>,
}

impl phy::RxToken for RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buffer[..])
    }
}

#[doc(hidden)]
pub struct TxToken {
    sender: UnboundedSender<(Vec<u8>, Sender<Option<IdeviceError>>)>,
}

impl phy::TxToken for TxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = vec![0; len];
        let result = f(&mut buffer);
        let (tx, rx) = channel();
        match self.sender.send((buffer, tx)) {
            Ok(_) => {
                if let Err(e) = rx.blocking_recv() {
                    warn!("Failed to send to idevice: {e:?}");
                }
            }
            Err(err) => warn!("Failed to send: {}", err),
        }
        result
    }
}
