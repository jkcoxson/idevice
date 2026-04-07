// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using CompanionProxyPtr =
    std::unique_ptr<CompanionProxyClientHandle,
                    FnDeleter<CompanionProxyClientHandle, companion_proxy_client_free>>;

class CompanionProxy {
  public:
    // Factory: connect via Provider
    static Result<CompanionProxy, FfiError>    connect(Provider& provider);

    // Factory: connect via RSD tunnel
    static Result<CompanionProxy, FfiError>    connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<CompanionProxy, FfiError>    from_socket(Idevice&& socket);

    // Ops
    Result<std::vector<std::string>, FfiError> get_device_registry();
    Result<uint16_t, FfiError>                 start_forwarding_service_port(uint16_t port);
    Result<void, FfiError>                     stop_forwarding_service_port(uint16_t port);

    // RAII / moves
    ~CompanionProxy() noexcept                                   = default;
    CompanionProxy(CompanionProxy&&) noexcept                    = default;
    CompanionProxy& operator=(CompanionProxy&&) noexcept         = default;
    CompanionProxy(const CompanionProxy&)                        = delete;
    CompanionProxy&             operator=(const CompanionProxy&) = delete;

    CompanionProxyClientHandle* raw() const noexcept { return handle_.get(); }
    static CompanionProxy       adopt(CompanionProxyClientHandle* h) noexcept {
        return CompanionProxy(h);
    }

  private:
    explicit CompanionProxy(CompanionProxyClientHandle* h) noexcept : handle_(h) {}
    CompanionProxyPtr handle_{};
};

} // namespace IdeviceFFI
