// Jackson Coxson

#ifndef IDEVICE_CORE_DEVICE_PROXY_H
#define IDEVICE_CORE_DEVICE_PROXY_H

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/readwrite.hpp>

namespace IdeviceFFI {

using CoreProxyPtr = std::unique_ptr<CoreDeviceProxyHandle,
                                     FnDeleter<CoreDeviceProxyHandle, core_device_proxy_free>>;
using AdapterPtr   = std::unique_ptr<AdapterHandle, FnDeleter<AdapterHandle, adapter_free>>;

struct CoreClientParams {
    uint16_t    mtu{};
    std::string address; // freed from Rust side after copy
    std::string netmask; // freed from Rust side after copy
};

class Adapter {
  public:
    ~Adapter() noexcept                      = default;
    Adapter(Adapter&&) noexcept              = default;
    Adapter& operator=(Adapter&&) noexcept   = default;
    Adapter(const Adapter&)                  = delete;
    Adapter&       operator=(const Adapter&) = delete;

    static Adapter adopt(AdapterHandle* h) noexcept { return Adapter(h); }
    AdapterHandle* raw() const noexcept { return handle_.get(); }

    // Enable PCAP
    bool           pcap(const std::string& path, FfiError& err) {
        if (IdeviceFfiError* e = ::adapter_pcap(handle_.get(), path.c_str())) {
            err = FfiError(e);
            return false;
        }
        return true;
    }

    // Connect to a port, returns a ReadWrite stream (to be consumed by RSD/CoreDeviceProxy)
    std::optional<ReadWrite> connect(uint16_t port, FfiError& err) {
        ReadWriteOpaque* s = nullptr;
        if (IdeviceFfiError* e = ::adapter_connect(handle_.get(), port, &s)) {
            err = FfiError(e);
            return std::nullopt;
        }
        return ReadWrite::adopt(s);
    }

  private:
    explicit Adapter(AdapterHandle* h) noexcept : handle_(h) {}
    AdapterPtr handle_{};
};

class CoreDeviceProxy {
  public:
    // Factory: connect using a Provider (NOT consumed on success or error)
    static std::optional<CoreDeviceProxy> connect(Provider& provider, FfiError& err);

    // Factory: from a socket; Rust consumes the socket regardless of result → we release before
    // call
    static std::optional<CoreDeviceProxy> from_socket(Idevice&& socket, FfiError& err);

    // Send/recv
    bool                                  send(const uint8_t* data, size_t len, FfiError& err);
    bool                                  send(const std::vector<uint8_t>& buf, FfiError& err) {
        return send(buf.data(), buf.size(), err);
    }

    // recv into a pre-sized buffer; resizes to actual bytes received
    bool                            recv(std::vector<uint8_t>& out, FfiError& err);

    // Handshake info
    std::optional<CoreClientParams> get_client_parameters(FfiError& err) const;
    std::optional<std::string>      get_server_address(FfiError& err) const;
    std::optional<uint16_t>         get_server_rsd_port(FfiError& err) const;

    // Consuming creation of a TCP adapter: Rust consumes the proxy handle
    std::optional<Adapter>          create_tcp_adapter(FfiError& err) &&;

    // RAII / moves
    ~CoreDeviceProxy() noexcept                              = default;
    CoreDeviceProxy(CoreDeviceProxy&&) noexcept              = default;
    CoreDeviceProxy& operator=(CoreDeviceProxy&&) noexcept   = default;
    CoreDeviceProxy(const CoreDeviceProxy&)                  = delete;
    CoreDeviceProxy&       operator=(const CoreDeviceProxy&) = delete;

    CoreDeviceProxyHandle* raw() const noexcept { return handle_.get(); }
    CoreDeviceProxyHandle* release() noexcept { return handle_.release(); }
    static CoreDeviceProxy adopt(CoreDeviceProxyHandle* h) noexcept { return CoreDeviceProxy(h); }

  private:
    explicit CoreDeviceProxy(CoreDeviceProxyHandle* h) noexcept : handle_(h) {}
    CoreProxyPtr handle_{};
};

} // namespace IdeviceFFI
#endif
