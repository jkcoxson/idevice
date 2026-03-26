// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/syslog_relay.hpp>

namespace IdeviceFFI {

Result<SyslogRelay, FfiError> SyslogRelay::connect_tcp(Provider& provider) {
    SyslogRelayClientHandle* out = nullptr;
    FfiError                 e(::syslog_relay_connect_tcp(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(SyslogRelay::adopt(out));
}

Result<std::string, FfiError> SyslogRelay::next() {
    char*    log_message = nullptr;
    FfiError e(::syslog_relay_next(handle_.get(), &log_message));
    if (e) {
        return Err(e);
    }
    std::string result;
    if (log_message) {
        result = log_message;
        ::idevice_string_free(log_message);
    }
    return Ok(std::move(result));
}

} // namespace IdeviceFFI
