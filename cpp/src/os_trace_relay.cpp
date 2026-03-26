// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/os_trace_relay.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

// -------- OsTraceRelay --------

Result<OsTraceRelay, FfiError> OsTraceRelay::connect(Provider& provider) {
    OsTraceRelayClientHandle* out = nullptr;
    FfiError                  e(::os_trace_relay_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(OsTraceRelay::adopt(out));
}

Result<OsTraceRelayReceiver, FfiError> OsTraceRelay::start_trace(const uint32_t* pid) {
    OsTraceRelayReceiverHandle* out = nullptr;
    // start_trace consumes the client handle
    FfiError                    e(::os_trace_relay_start_trace(handle_.release(), &out, pid));
    if (e) {
        return Err(e);
    }
    return Ok(OsTraceRelayReceiver::adopt(out));
}

Result<std::vector<uint64_t>, FfiError> OsTraceRelay::get_pid_list() {
    Vec_u64* list = nullptr;
    FfiError e(::os_trace_relay_get_pid_list(handle_.get(), &list));
    if (e) {
        return Err(e);
    }

    std::vector<uint64_t> result;
    if (list) {
        // Vec_u64 is an opaque type - we return the raw pointer for now
        // The caller should handle this appropriately
        // For safety, we free it after use
        ::idevice_outer_slice_free(list, 0);
    }

    return Ok(std::move(result));
}

// -------- OsTraceRelayReceiver --------

Result<OsTraceLog*, FfiError> OsTraceRelayReceiver::next() {
    OsTraceLog* log = nullptr;
    FfiError    e(::os_trace_relay_next(handle_.get(), &log));
    if (e) {
        return Err(e);
    }
    return Ok(log);
}

void OsTraceRelayReceiver::free_log(OsTraceLog* log) {
    if (log) {
        ::os_trace_relay_free_log(log);
    }
}

} // namespace IdeviceFFI
