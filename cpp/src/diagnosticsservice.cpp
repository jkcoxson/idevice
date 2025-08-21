// Jackson Coxson

#include <idevice++/diagnosticsservice.hpp>
#include <cstring>

namespace IdeviceFFI {

// Local helper: take ownership of a C string and convert to std::string
static std::optional<std::string> take_cstring(char* p) {
    if (!p)
        return std::nullopt;
    std::string s(p);
    ::idevice_string_free(p);
    return s;
}

// -------- SysdiagnoseStream --------
std::optional<std::vector<uint8_t>> SysdiagnoseStream::next_chunk(FfiError& err) {
    if (!h_)
        return std::nullopt;

    uint8_t*    data = nullptr;
    std::size_t len  = 0;

    if (IdeviceFfiError* e = ::sysdiagnose_stream_next(h_, &data, &len)) {
        err = FfiError(e);
        return std::nullopt;
    }

    if (!data || len == 0) {
        // End of stream
        return std::nullopt;
    }

    // Copy into a C++ buffer
    std::vector<uint8_t> out(len);
    std::memcpy(out.data(), data, len);

    idevice_data_free(data, len);

    return out;
}

// -------- DiagnosticsService --------
std::optional<DiagnosticsService>
DiagnosticsService::connect_rsd(Adapter& adapter, RsdHandshake& rsd, FfiError& err) {
    ::DiagnosticsServiceHandle* out = nullptr;
    if (IdeviceFfiError* e = ::diagnostics_service_connect_rsd(adapter.raw(), rsd.raw(), &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return DiagnosticsService(out);
}

std::optional<DiagnosticsService> DiagnosticsService::from_stream_ptr(::ReadWriteOpaque* consumed,
                                                                      FfiError&          err) {
    ::DiagnosticsServiceHandle* out = nullptr;
    if (IdeviceFfiError* e = ::diagnostics_service_new(consumed, &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return DiagnosticsService(out);
}

std::optional<SysdiagnoseCapture> DiagnosticsService::capture_sysdiagnose(bool      dry_run,
                                                                          FfiError& err) {
    if (!h_)
        return std::nullopt;

    char*                      filename_c   = nullptr;
    std::size_t                expected_len = 0;
    ::SysdiagnoseStreamHandle* stream_h     = nullptr;

    if (IdeviceFfiError* e = ::diagnostics_service_capture_sysdiagnose(
            h_, dry_run ? true : false, &filename_c, &expected_len, &stream_h)) {
        err = FfiError(e);
        return std::nullopt;
    }

    auto               fname = take_cstring(filename_c).value_or(std::string{});
    SysdiagnoseStream  stream(stream_h);

    SysdiagnoseCapture cap{/*preferred_filename*/ std::move(fname),
                           /*expected_length*/ expected_len,
                           /*stream*/ std::move(stream)};
    return cap;
}

} // namespace IdeviceFFI
