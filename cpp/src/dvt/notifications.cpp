// Jackson Coxson

#include <idevice++/dvt/notifications.hpp>

namespace IdeviceFFI {

Result<Notifications, FfiError> Notifications::create(RemoteServer& server) {
    NotificationsHandle* out = nullptr;
    FfiError             e(::notifications_new(server.raw(), &out));
    if (e) return Err(e);
    return Ok(Notifications::adopt(out));
}

Result<void, FfiError> Notifications::start() {
    FfiError e(::notifications_start(handle_.get()));
    if (e) return Err(e);
    return Ok();
}

Result<void, FfiError> Notifications::stop() {
    FfiError e(::notifications_stop(handle_.get()));
    if (e) return Err(e);
    return Ok();
}

Result<NotificationInfo, FfiError> Notifications::next_notification() {
    IdeviceNotificationInfo* raw = nullptr;
    FfiError                 e(::notifications_get_next(handle_.get(), &raw));
    if (e) return Err(e);

    NotificationInfo info{};
    if (raw) {
        info.notification_type =
            raw->notification_type ? std::string(raw->notification_type) : std::string();
        info.mach_absolute_time = raw->mach_absolute_time;
        info.exec_name          = raw->exec_name ? std::string(raw->exec_name) : std::string();
        info.app_name           = raw->app_name ? std::string(raw->app_name) : std::string();
        info.pid                = raw->pid;
        info.state_description =
            raw->state_description ? std::string(raw->state_description) : std::string();
    }

    ::notifications_info_free(raw);
    return Ok(std::move(info));
}

} // namespace IdeviceFFI
