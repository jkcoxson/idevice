// Jackson Coxson

#include <cstring>
#include <idevice++/diagnosticsservice.hpp>
#include <idevice++/option.hpp>

namespace IdeviceFFI {

// Local helper: take ownership of a C string and convert to std::string
static Option<std::string> take_cstring(char* p) {
    if (!p) {
        return None;
    }

    std::string s(p);
    ::idevice_string_free(p);
    return Some(std::move(s));
}

// -------- SysdiagnoseStream --------
Result<Option<std::vector<uint8_t>>, FfiError> SysdiagnoseStream::next_chunk() {
    if (!h_) {
        return Err(FfiError::NotConnected());
    }

    uint8_t*    data = nullptr;
    std::size_t len  = 0;

    FfiError    e(::sysdiagnose_stream_next(h_, &data, &len));
    if (e) {
        return Err(e);
    }

    if (!data || len == 0) {
        // End of stream
        return Ok(Option<std::vector<uint8_t>>(None));
    }

    // Copy into a C++ buffer
    std::vector<uint8_t> out(len);
    std::memcpy(out.data(), data, len);

    idevice_data_free(data, len);

    return Ok(Some(out));
}

// -------- DiagnosticsService --------
Result<DiagnosticsService, FfiError> DiagnosticsService::connect_rsd(Adapter&      adapter,
                                                                     RsdHandshake& rsd) {
    ::DiagnosticsServiceHandle* out = nullptr;
    FfiError e(::diagnostics_service_connect_rsd(adapter.raw(), rsd.raw(), &out));
    if (e) {
        return Err(e);
    }
    return Ok(DiagnosticsService(out));
}

Result<DiagnosticsService, FfiError>
DiagnosticsService::from_stream_ptr(::ReadWriteOpaque* consumed) {
    ::DiagnosticsServiceHandle* out = nullptr;
    FfiError                    e(::diagnostics_service_new(consumed, &out));
    if (e) {
        return Err(e);
    }
    return Ok(DiagnosticsService(out));
}

Result<SysdiagnoseCapture, FfiError> DiagnosticsService::capture_sysdiagnose(bool dry_run) {
    if (!h_) {
        return Err(FfiError::NotConnected());
    }

    char*                      filename_c   = nullptr;
    std::size_t                expected_len = 0;
    ::SysdiagnoseStreamHandle* stream_h     = nullptr;

    FfiError                   e(::diagnostics_service_capture_sysdiagnose(
        h_, dry_run ? true : false, &filename_c, &expected_len, &stream_h));
    if (e) {
        return Err(e);
    }

    auto               fname = take_cstring(filename_c).unwrap_or(std::string{});
    SysdiagnoseStream  stream(stream_h);

    SysdiagnoseCapture cap{/*preferred_filename*/ std::move(fname),
                           /*expected_length*/ expected_len,
                           /*stream*/ std::move(stream)};
    return Ok(std::move(cap));
}

} // namespace IdeviceFFI
