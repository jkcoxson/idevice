// Jackson Coxson

#ifndef IDEVICE_CORE_DEVICE_PROXY_H
#define IDEVICE_CORE_DEVICE_PROXY_H

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/option.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/result.hpp>

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
    ~Adapter() noexcept                              = default;
    Adapter(Adapter&&) noexcept                      = default;
    Adapter& operator=(Adapter&&) noexcept           = default;
    Adapter(const Adapter&)                          = delete;
    Adapter&               operator=(const Adapter&) = delete;

    static Adapter         adopt(AdapterHandle* h) noexcept { return Adapter(h); }
    AdapterHandle*         raw() const noexcept { return handle_.get(); }

    // Enable PCAP
    Result<void, FfiError> pcap(const std::string& path) {
        FfiError e(::adapter_pcap(handle_.get(), path.c_str()));
        if (e) {
            return Err(e);
        }
        return Ok();
    }

    // Connect to a port, returns a ReadWrite stream (to be consumed by
    // RSD/CoreDeviceProxy)
    Result<ReadWrite, FfiError> connect(uint16_t port) {
        ReadWriteOpaque* s = nullptr;
        FfiError         e(::adapter_connect(handle_.get(), port, &s));
        if (e) {
            return Err(e);
        }
        return Ok(ReadWrite::adopt(s));
    }

    Result<void, FfiError> close() {
        FfiError e(::adapter_close(handle_.get()));
        if (e) {
            return Err(e);
        }
        return Ok();
    }

  private:
    explicit Adapter(AdapterHandle* h) noexcept : handle_(h) {}
    AdapterPtr handle_{};
};

class CoreDeviceProxy {
  public:
    // Factory: connect using a Provider (NOT consumed on success or error)
    static Result<CoreDeviceProxy, FfiError> connect(Provider& provider);

    // Factory: from a socket; Rust consumes the socket regardless of result → we
    // release before call
    static Result<CoreDeviceProxy, FfiError> from_socket(Idevice&& socket);

    // Send/recv
    Result<void, FfiError>                   send(const uint8_t* data, size_t len);
    Result<void, FfiError>                   send(const std::vector<uint8_t>& buf) {
        return send(buf.data(), buf.size());
    }

    // recv into a pre-sized buffer; resizes to actual bytes received
    Result<void, FfiError>             recv(std::vector<uint8_t>& out);

    // Handshake info
    Result<CoreClientParams, FfiError> get_client_parameters() const;
    Result<std::string, FfiError>      get_server_address() const;
    Result<uint16_t, FfiError>         get_server_rsd_port() const;

    // Consuming creation of a TCP adapter: Rust consumes the proxy handle
    Result<Adapter, FfiError>          create_tcp_adapter() &&;

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
