// Jackson Coxson

#include <idevice++/ffi.hpp>
#include <idevice++/usbmuxd.hpp>

namespace IdeviceFFI {

// ---------- UsbmuxdAddr ----------
Result<UsbmuxdAddr, FfiError> UsbmuxdAddr::tcp_new(const sockaddr* addr, socklen_t addr_len) {
    UsbmuxdAddrHandle* h = nullptr;
    FfiError           e(idevice_usbmuxd_tcp_addr_new(addr, addr_len, &h));
    if (e) {
        return Err(e);
    }
    return Ok(UsbmuxdAddr(h));
}

#if defined(__unix__) || defined(__APPLE__)
Result<UsbmuxdAddr, FfiError> UsbmuxdAddr::unix_new(const std::string& path) {
    UsbmuxdAddrHandle* h = nullptr;
    FfiError           e(idevice_usbmuxd_unix_addr_new(path.c_str(), &h));
    if (e) {
        return Err(e);
    }
    return Ok(UsbmuxdAddr(h));
}
#endif

UsbmuxdAddr UsbmuxdAddr::default_new() {
    UsbmuxdAddrHandle* h = nullptr;
    idevice_usbmuxd_default_addr_new(&h);
    return UsbmuxdAddr::adopt(h);
}

// ---------- UsbmuxdConnectionType ----------
std::string UsbmuxdConnectionType::to_string() const {
    switch (_value) {
    case Value::Usb:
        return "USB";
    case Value::Network:
        return "Network";
    case Value::Unknown:
        return "Unknown";
    default:
        return "UnknownEnumValue";
    }
}

// ---------- UsbmuxdDevice ----------
Option<std::string> UsbmuxdDevice::get_udid() const {
    char* c = idevice_usbmuxd_device_get_udid(handle_.get());
    if (!c) {
        return None;
    }
    std::string out(c);
    idevice_string_free(c);
    return Some(out);
}

Option<uint32_t> UsbmuxdDevice::get_id() const {
    uint32_t id = idevice_usbmuxd_device_get_device_id(handle_.get());
    if (id == 0) {
        return None;
    }
    return Some(id);
}

Option<UsbmuxdConnectionType> UsbmuxdDevice::get_connection_type() const {
    uint8_t t = idevice_usbmuxd_device_get_connection_type(handle_.get());
    if (t == 0) {
        return None;
    }
    return Some(UsbmuxdConnectionType(t));
}

// ---------- UsbmuxdConnection ----------
Result<UsbmuxdConnection, FfiError>
UsbmuxdConnection::tcp_new(const idevice_sockaddr* addr, idevice_socklen_t addr_len, uint32_t tag) {
    UsbmuxdConnectionHandle* h = nullptr;
    FfiError                 e(idevice_usbmuxd_new_tcp_connection(addr, addr_len, tag, &h));
    if (e) {
        return Err(e);
    }
    return Ok(UsbmuxdConnection(h));
}

#if defined(__unix__) || defined(__APPLE__)
Result<UsbmuxdConnection, FfiError> UsbmuxdConnection::unix_new(const std::string& path,
                                                                uint32_t           tag) {
    UsbmuxdConnectionHandle* h = nullptr;
    FfiError                 e(idevice_usbmuxd_new_unix_socket_connection(path.c_str(), tag, &h));
    if (e) {
        return Err(e);
    }
    return Ok(UsbmuxdConnection(h));
}
#endif

Result<UsbmuxdConnection, FfiError> UsbmuxdConnection::default_new(uint32_t tag) {
    UsbmuxdConnectionHandle* h = nullptr;
    FfiError                 e(idevice_usbmuxd_new_default_connection(tag, &h));
    if (e) {
        return Err(e);
    }
    return Ok(UsbmuxdConnection(h));
}

Result<std::vector<UsbmuxdDevice>, FfiError> UsbmuxdConnection::get_devices() const {
    UsbmuxdDeviceHandle** list  = nullptr;
    int                   count = 0;
    FfiError              e(idevice_usbmuxd_get_devices(handle_.get(), &list, &count));
    if (e) {
        return Err(e);
    }
    std::vector<UsbmuxdDevice> out;
    out.reserve(count);
    for (int i = 0; i < count; ++i) {
        out.emplace_back(UsbmuxdDevice::adopt(list[i]));
    }
    idevice_outer_slice_free(list, count);
    return Ok(std::move(out));
}

Result<std::string, FfiError> UsbmuxdConnection::get_buid() const {
    char*    c = nullptr;
    FfiError e(idevice_usbmuxd_get_buid(handle_.get(), &c));
    if (e) {
        return Err(e);
    }
    std::string out(c);
    idevice_string_free(c);
    return Ok(out);
}

Result<PairingFile, FfiError> UsbmuxdConnection::get_pair_record(const std::string& udid) {
    IdevicePairingFile* pf = nullptr;
    FfiError            e(idevice_usbmuxd_get_pair_record(handle_.get(), udid.c_str(), &pf));
    if (e) {
        return Err(e);
    }
    return Ok(PairingFile(pf));
}

Result<Idevice, FfiError> UsbmuxdConnection::connect_to_device(uint32_t           device_id,
                                                               uint16_t           port,
                                                               const std::string& path) && {
    UsbmuxdConnectionHandle* raw = handle_.release();
    IdeviceHandle*           out = nullptr;
    FfiError e(idevice_usbmuxd_connect_to_device(raw, device_id, port, path.c_str(), &out));
    if (e) {
        return Err(e);
    }
    return Ok(Idevice::adopt(out));
}

} // namespace IdeviceFFI
