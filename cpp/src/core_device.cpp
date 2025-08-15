// Jackson Coxson

#include <idevice++/core_device_proxy.hpp>

namespace IdeviceFFI {

// ---- Factories ----

std::optional<CoreDeviceProxy> CoreDeviceProxy::connect(Provider& provider, FfiError& err) {
    CoreDeviceProxyHandle* out = nullptr;
    if (IdeviceFfiError* e = ::core_device_proxy_connect(provider.raw(), &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return CoreDeviceProxy::adopt(out);
}

std::optional<CoreDeviceProxy> CoreDeviceProxy::from_socket(Idevice&& socket, FfiError& err) {
    CoreDeviceProxyHandle* out = nullptr;

    // Rust consumes the socket regardless of result → release BEFORE call
    IdeviceHandle*         raw = socket.release();

    if (IdeviceFfiError* e = ::core_device_proxy_new(raw, &out)) {
        // socket is already consumed on error; do NOT touch it
        err = FfiError(e);
        return std::nullopt;
    }
    return CoreDeviceProxy::adopt(out);
}

// ---- IO ----

bool CoreDeviceProxy::send(const uint8_t* data, size_t len, FfiError& err) {
    if (IdeviceFfiError* e = ::core_device_proxy_send(handle_.get(), data, len)) {
        err = FfiError(e);
        return false;
    }
    return true;
}

bool CoreDeviceProxy::recv(std::vector<uint8_t>& out, FfiError& err) {
    if (out.empty())
        out.resize(4096); // a reasonable default; caller can pre-size
    size_t actual = 0;
    if (IdeviceFfiError* e =
            ::core_device_proxy_recv(handle_.get(), out.data(), &actual, out.size())) {
        err = FfiError(e);
        return false;
    }
    out.resize(actual);
    return true;
}

// ---- Handshake ----

std::optional<CoreClientParams> CoreDeviceProxy::get_client_parameters(FfiError& err) const {
    uint16_t mtu    = 0;
    char*    addr_c = nullptr;
    char*    mask_c = nullptr;

    if (IdeviceFfiError* e =
            ::core_device_proxy_get_client_parameters(handle_.get(), &mtu, &addr_c, &mask_c)) {
        err = FfiError(e);
        return std::nullopt;
    }

    CoreClientParams params;
    params.mtu = mtu;
    if (addr_c) {
        params.address = addr_c;
        ::idevice_string_free(addr_c);
    }
    if (mask_c) {
        params.netmask = mask_c;
        ::idevice_string_free(mask_c);
    }
    return params;
}

std::optional<std::string> CoreDeviceProxy::get_server_address(FfiError& err) const {
    char* addr_c = nullptr;
    if (IdeviceFfiError* e = ::core_device_proxy_get_server_address(handle_.get(), &addr_c)) {
        err = FfiError(e);
        return std::nullopt;
    }
    std::string s;
    if (addr_c) {
        s = addr_c;
        ::idevice_string_free(addr_c);
    }
    return s;
}

std::optional<uint16_t> CoreDeviceProxy::get_server_rsd_port(FfiError& err) const {
    uint16_t port = 0;
    if (IdeviceFfiError* e = ::core_device_proxy_get_server_rsd_port(handle_.get(), &port)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return port;
}

// ---- Adapter creation (consumes *this) ----

std::optional<Adapter> CoreDeviceProxy::create_tcp_adapter(FfiError& err) && {
    AdapterHandle*         out = nullptr;

    // Rust consumes the proxy regardless of result → release BEFORE call
    CoreDeviceProxyHandle* raw = this->release();

    if (IdeviceFfiError* e = ::core_device_proxy_create_tcp_adapter(raw, &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return Adapter::adopt(out);
}

} // namespace IdeviceFFI
