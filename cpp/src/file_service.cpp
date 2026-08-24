// Jackson Coxson

#include <idevice++/file_service.hpp>

namespace IdeviceFFI {

const char* FileService::CONTROL_SERVICE_NAME = "com.apple.coredevice.fileservice.control";
const char* FileService::DATA_SERVICE_NAME    = "com.apple.coredevice.fileservice.data";

Option<FileServiceDomain> file_service_domain_from_name(const std::string& name) {
    ::IdeviceFileServiceDomain domain = IdeviceFileServiceDomainAppDataContainer;
    if (!::file_service_domain_from_name(name.c_str(), &domain)) {
        return None;
    }
    return Option<FileServiceDomain>(static_cast<FileServiceDomain>(domain));
}

// ---- Factories ----
Result<FileService, FfiError> FileService::connect_rsd(Adapter& adapter, RsdHandshake& rsd) {
    FileServiceHandle* out = nullptr;
    if (IdeviceFfiError* e = ::file_service_connect_rsd(adapter.raw(), rsd.raw(), &out)) {
        return Err(FfiError(e));
    }
    return Ok(FileService::adopt(out));
}

Result<FileService, FfiError> FileService::from_readwrite_ptr(ReadWriteOpaque* consumed) {
    FileServiceHandle* out = nullptr;
    if (IdeviceFfiError* e = ::file_service_new(consumed, &out)) {
        return Err(FfiError(e));
    }
    return Ok(FileService::adopt(out));
}

Result<FileService, FfiError> FileService::from_readwrite(ReadWrite&& rw) {
    // Rust consumes the stream regardless of result → release BEFORE call
    return from_readwrite_ptr(rw.release());
}

// ---- API impls ----
Result<std::string, FfiError> FileService::create_session(FileServiceDomain  domain,
                                                          const std::string& identifier) {
    char* session = nullptr;
    if (IdeviceFfiError* e =
            ::file_service_create_session(handle_.get(),
                                          static_cast<::IdeviceFileServiceDomain>(domain),
                                          identifier.c_str(),
                                          &session)) {
        return Err(FfiError(e));
    }
    std::string out;
    if (session) {
        out = session;
        ::idevice_string_free(session);
    }
    return Ok(std::move(out));
}

Result<Option<std::string>, FfiError> FileService::session_id() const {
    char* session = nullptr;
    if (IdeviceFfiError* e = ::file_service_session_id(handle_.get(), &session)) {
        return Err(FfiError(e));
    }
    Option<std::string> out;
    if (session) {
        out = std::string(session);
        ::idevice_string_free(session);
    }
    return Ok(std::move(out));
}

Result<std::vector<std::string>, FfiError> FileService::list_directory(const std::string& path) {
    char** entries = nullptr;
    size_t n       = 0;
    if (IdeviceFfiError* e =
            ::file_service_retrieve_directory_list(handle_.get(), path.c_str(), &entries, &n)) {
        return Err(FfiError(e));
    }

    std::vector<std::string> out;
    out.reserve(n);
    for (size_t i = 0; i < n; ++i) {
        if (entries[i]) {
            out.emplace_back(entries[i]);
        }
    }
    ::file_service_free_directory_list(entries, n);
    return Ok(std::move(out));
}

Result<std::vector<uint8_t>, FfiError>
FileService::retrieve_file(const std::string& path, Adapter& adapter, uint16_t data_port) {
    uint8_t* data = nullptr;
    size_t   n    = 0;
    if (IdeviceFfiError* e = ::file_service_retrieve_file(handle_.get(),
                                                          path.c_str(),
                                                          adapter.raw(),
                                                          data_port,
                                                          &data,
                                                          &n)) {
        return Err(FfiError(e));
    }

    std::vector<uint8_t> out;
    if (data && n) {
        out.assign(data, data + n);
    }
    ::idevice_data_free(data, n);
    return Ok(std::move(out));
}

Result<std::vector<uint8_t>, FfiError> FileService::retrieve_file(const std::string& path,
                                                                  ReadWrite&& data_stream) {
    uint8_t* data = nullptr;
    size_t   n    = 0;
    // Rust consumes the stream regardless of result → release BEFORE call
    if (IdeviceFfiError* e = ::file_service_retrieve_file_with_stream(handle_.get(),
                                                                      path.c_str(),
                                                                      data_stream.release(),
                                                                      &data,
                                                                      &n)) {
        return Err(FfiError(e));
    }

    std::vector<uint8_t> out;
    if (data && n) {
        out.assign(data, data + n);
    }
    ::idevice_data_free(data, n);
    return Ok(std::move(out));
}

Result<void, FfiError> FileService::propose_empty_file(const std::string& path,
                                                       uint32_t           file_permissions,
                                                       uint32_t           uid,
                                                       uint32_t           gid,
                                                       int64_t            creation_time,
                                                       int64_t            last_modification_time) {
    if (IdeviceFfiError* e = ::file_service_propose_empty_file(handle_.get(),
                                                               path.c_str(),
                                                               file_permissions,
                                                               uid,
                                                               gid,
                                                               creation_time,
                                                               last_modification_time)) {
        return Err(FfiError(e));
    }
    return Ok();
}

} // namespace IdeviceFFI
