// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using NotificationsPtr =
    std::unique_ptr<NotificationsHandle, FnDeleter<NotificationsHandle, notifications_free>>;

struct NotificationInfo {
    std::string notification_type;
    int64_t     mach_absolute_time = 0;
    std::string exec_name;
    std::string app_name;
    uint32_t    pid                = 0;
    std::string state_description;
};

class Notifications {
  public:
    static Result<Notifications, FfiError> create(RemoteServer& server);

    Result<void, FfiError>             start();
    Result<void, FfiError>             stop();
    Result<NotificationInfo, FfiError> next_notification();

    ~Notifications() noexcept                          = default;
    Notifications(Notifications&&) noexcept            = default;
    Notifications& operator=(Notifications&&) noexcept = default;
    Notifications(const Notifications&)                = delete;
    Notifications& operator=(const Notifications&)     = delete;

    NotificationsHandle* raw() const noexcept { return handle_.get(); }
    static Notifications adopt(NotificationsHandle* h) noexcept {
        return Notifications(h);
    }

  private:
    explicit Notifications(NotificationsHandle* h) noexcept : handle_(h) {}
    NotificationsPtr handle_{};
};

} // namespace IdeviceFFI
