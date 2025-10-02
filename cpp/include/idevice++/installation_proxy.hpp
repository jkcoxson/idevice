#pragma once
#include <functional>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <sys/_types/_u_int64_t.h>

namespace IdeviceFFI {

using InstallationProxyPtr =
    std::unique_ptr<InstallationProxyClientHandle,
                    FnDeleter<InstallationProxyClientHandle, installation_proxy_client_free>>;

class InstallationProxy {
  public:
    // Factory: connect via Provider
    static Result<InstallationProxy, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<InstallationProxy, FfiError> from_socket(Idevice&& socket);

    // Ops
    Result<std::vector<plist_t>, FfiError>
                           get_apps(Option<std::string>              application_type,
                                    Option<std::vector<std::string>> bundle_identifiers);
    Result<void, FfiError> install(std::string package_path, Option<plist_t> options);
    Result<void, FfiError> install_with_callback(std::string                     package_path,
                                                 Option<plist_t>                 options,
                                                 std::function<void(u_int64_t)>& lambda);
    Result<void, FfiError> upgrade(std::string package_path, Option<plist_t> options);
    Result<void, FfiError> upgrade_with_callback(std::string                     package_path,
                                                 Option<plist_t>                 options,
                                                 std::function<void(u_int64_t)>& lambda);
    Result<void, FfiError> uninstall(std::string package_path, Option<plist_t> options);
    Result<void, FfiError> uninstall_with_callback(std::string                     package_path,
                                                   Option<plist_t>                 options,
                                                   std::function<void(u_int64_t)>& lambda);
    Result<bool, FfiError> check_capabilities_match(std::vector<plist_t> capabilities,
                                                    Option<plist_t>      options);
    Result<std::vector<plist_t>, FfiError> browse(Option<plist_t> options);

    // RAII / moves
    ~InstallationProxy() noexcept                                      = default;
    InstallationProxy(InstallationProxy&&) noexcept                    = default;
    InstallationProxy& operator=(InstallationProxy&&) noexcept         = default;
    InstallationProxy(const InstallationProxy&)                        = delete;
    InstallationProxy&             operator=(const InstallationProxy&) = delete;

    InstallationProxyClientHandle* raw() const noexcept { return handle_.get(); }
    static InstallationProxy       adopt(InstallationProxyClientHandle* h) noexcept {
        return InstallationProxy(h);
    }

  private:
    explicit InstallationProxy(InstallationProxyClientHandle* h) noexcept : handle_(h) {}
    InstallationProxyPtr handle_{};
};

} // namespace IdeviceFFI
