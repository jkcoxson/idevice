#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using NotificationProxyPtr = std::unique_ptr<NotificationProxyClientHandle,
    FnDeleter<NotificationProxyClientHandle, notification_proxy_client_free>>;

class NotificationProxy {
  public:
    // Factory: connect via Provider
    static Result<NotificationProxy, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<NotificationProxy, FfiError> from_socket(Idevice&& socket);

    // Ops
    Result<void, FfiError>        post_notification(const std::string& name);
    Result<void, FfiError>        observe_notification(const std::string& name);
    Result<void, FfiError>        observe_notifications(const std::vector<std::string>& names);
    Result<std::string, FfiError> receive_notification();
    Result<std::string, FfiError> receive_notification_with_timeout(u_int64_t interval);

    // RAII / moves
    ~NotificationProxy() noexcept                                      = default;
    NotificationProxy(NotificationProxy&&) noexcept                    = default;
    NotificationProxy& operator=(NotificationProxy&&) noexcept         = default;
    NotificationProxy(const NotificationProxy&)                        = delete;
    NotificationProxy& operator=(const NotificationProxy&)             = delete;

    NotificationProxyClientHandle* raw() const noexcept { return handle_.get(); }
    static NotificationProxy adopt(NotificationProxyClientHandle* h) noexcept {
        return NotificationProxy(h);
    }

  private:
    explicit NotificationProxy(NotificationProxyClientHandle* h) noexcept : handle_(h) {}
    NotificationProxyPtr handle_{};
};

} // namespace IdeviceFFI
