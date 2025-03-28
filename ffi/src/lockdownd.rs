use log::error;
use plist::Value;
use serde::{Deserialize, Serialize};
use idevice::{IdeviceError, IdeviceService, lockdownd::LockdowndClient};

use crate::{pairing_file, Idevice, IdeviceError, IdeviceService};
pub struct LockdowndClientHandle(pub LockdowndClient);
