// Jackson Coxson

#include <idevice++/ffi.hpp>
#include <idevice++/usbmuxd.hpp>

namespace IdeviceFFI {

// ---------- UsbmuxdAddr ----------
std::optional<UsbmuxdAddr>
UsbmuxdAddr::tcp_new(const sockaddr* addr, socklen_t addr_len, FfiError& err) {
    UsbmuxdAddrHandle* h = nullptr;
    if (IdeviceFfiError* e = idevice_usbmuxd_tcp_addr_new(addr, addr_len, &h)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return UsbmuxdAddr(h);
}

#if defined(__unix__) || defined(__APPLE__)
std::optional<UsbmuxdAddr> UsbmuxdAddr::unix_new(const std::string& path, FfiError& err) {
    UsbmuxdAddrHandle* h = nullptr;
    if (IdeviceFfiError* e = idevice_usbmuxd_unix_addr_new(path.c_str(), &h)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return UsbmuxdAddr(h);
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
std::optional<std::string> UsbmuxdDevice::get_udid() const {
    char* c = idevice_usbmuxd_device_get_udid(handle_.get());
    if (!c)
        return std::nullopt;
    std::string out(c);
    idevice_string_free(c);
    return out;
}

std::optional<uint32_t> UsbmuxdDevice::get_id() const {
    uint32_t id = idevice_usbmuxd_device_get_device_id(handle_.get());
    if (id == 0)
        return std::nullopt; // adjust if 0 can be valid
    return id;
}

std::optional<UsbmuxdConnectionType> UsbmuxdDevice::get_connection_type() const {
    uint8_t t = idevice_usbmuxd_device_get_connection_type(handle_.get());
    if (t == 0)
        return std::nullopt;
    return UsbmuxdConnectionType(t);
}

// ---------- UsbmuxdConnection ----------
std::optional<UsbmuxdConnection> UsbmuxdConnection::tcp_new(const idevice_sockaddr* addr,
                                                            idevice_socklen_t       addr_len,
                                                            uint32_t                tag,
                                                            FfiError&               err) {
    UsbmuxdConnectionHandle* h = nullptr;
    if (IdeviceFfiError* e = idevice_usbmuxd_new_tcp_connection(addr, addr_len, tag, &h)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return UsbmuxdConnection(h);
}

#if defined(__unix__) || defined(__APPLE__)
std::optional<UsbmuxdConnection>
UsbmuxdConnection::unix_new(const std::string& path, uint32_t tag, FfiError& err) {
    UsbmuxdConnectionHandle* h = nullptr;
    if (IdeviceFfiError* e = idevice_usbmuxd_new_unix_socket_connection(path.c_str(), tag, &h)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return UsbmuxdConnection(h);
}
#endif

std::optional<UsbmuxdConnection> UsbmuxdConnection::default_new(uint32_t tag, FfiError& err) {
    UsbmuxdConnectionHandle* h = nullptr;
    if (IdeviceFfiError* e = idevice_usbmuxd_new_default_connection(tag, &h)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return UsbmuxdConnection(h);
}

std::optional<std::vector<UsbmuxdDevice>> UsbmuxdConnection::get_devices(FfiError& err) const {
    UsbmuxdDeviceHandle** list  = nullptr;
    int                   count = 0;
    if (IdeviceFfiError* e = idevice_usbmuxd_get_devices(handle_.get(), &list, &count)) {
        err = FfiError(e);
        return std::nullopt;
    }
    std::vector<UsbmuxdDevice> out;
    out.reserve(count);
    for (int i = 0; i < count; ++i) {
        out.emplace_back(UsbmuxdDevice::adopt(list[i]));
    }
    return out;
}

std::optional<std::string> UsbmuxdConnection::get_buid(FfiError& err) const {
    char* c = nullptr;
    if (IdeviceFfiError* e = idevice_usbmuxd_get_buid(handle_.get(), &c)) {
        err = FfiError(e);
        return std::nullopt;
    }
    std::string out(c);
    idevice_string_free(c);
    return out;
}

std::optional<PairingFile> UsbmuxdConnection::get_pair_record(const std::string& udid,
                                                              FfiError&          err) {
    IdevicePairingFile* pf = nullptr;
    if (IdeviceFfiError* e = idevice_usbmuxd_get_pair_record(handle_.get(), udid.c_str(), &pf)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return PairingFile(pf);
}

std::optional<Idevice> UsbmuxdConnection::connect_to_device(uint32_t           device_id,
                                                            uint16_t           port,
                                                            const std::string& path,
                                                            FfiError&          err) && {
    UsbmuxdConnectionHandle* raw = handle_.release();

    IdeviceHandle*           out = nullptr;
    IdeviceFfiError*         e =
        idevice_usbmuxd_connect_to_device(raw, device_id, port, path.c_str(), &out);

    if (e) {
        err = FfiError(e);
        return std::nullopt;
    }
    return Idevice::adopt(out);
}

} // namespace IdeviceFFI
