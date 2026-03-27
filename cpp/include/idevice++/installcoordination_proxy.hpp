// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/idevice.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using InstallcoordinationProxyPtr = std::unique_ptr<
    InstallcoordinationProxyHandle,
    FnDeleter<InstallcoordinationProxyHandle, installcoordination_proxy_client_free>>;

class InstallcoordinationProxy {
  public:
    // Factory: connect via RSD tunnel
    static Result<InstallcoordinationProxy, FfiError> connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake);

    // Factory: from ReadWrite stream (RSD-only, consumes the pointer)
    static Result<InstallcoordinationProxy, FfiError> from_readwrite_ptr(ReadWriteOpaque* consumed);

    // Ergonomic overload: consume a C++ ReadWrite
    static Result<InstallcoordinationProxy, FfiError> from_readwrite(ReadWrite&& rw);

    // Ops
    Result<void, FfiError>                            uninstall_app(const std::string& bundle_id);
    Result<std::string, FfiError>                     query_app_path(const std::string& bundle_id);

    // RAII / moves
    ~InstallcoordinationProxy() noexcept                                       = default;
    InstallcoordinationProxy(InstallcoordinationProxy&&) noexcept              = default;
    InstallcoordinationProxy& operator=(InstallcoordinationProxy&&) noexcept   = default;
    InstallcoordinationProxy(const InstallcoordinationProxy&)                  = delete;
    InstallcoordinationProxy&       operator=(const InstallcoordinationProxy&) = delete;

    InstallcoordinationProxyHandle* raw() const noexcept { return handle_.get(); }
    static InstallcoordinationProxy adopt(InstallcoordinationProxyHandle* h) noexcept {
        return InstallcoordinationProxy(h);
    }

  private:
    explicit InstallcoordinationProxy(InstallcoordinationProxyHandle* h) noexcept : handle_(h) {}
    InstallcoordinationProxyPtr handle_{};
};

} // namespace IdeviceFFI
