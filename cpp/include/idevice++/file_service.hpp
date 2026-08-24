// Jackson Coxson

#pragma once
#include <cstddef>
#include <cstdint>
#include <idevice++/adapter_stream.hpp>
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/option.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/rsd.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using FileServicePtr =
    std::unique_ptr<FileServiceHandle, FnDeleter<FileServiceHandle, file_service_free>>;

// Which of the device's filesystem domains a session is scoped to.
enum class FileServiceDomain {
    // An app's own data container. The identifier is the bundle ID.
    AppDataContainer      = IdeviceFileServiceDomainAppDataContainer,
    // A shared app-group container. The identifier is the group ID.
    AppGroupDataContainer = IdeviceFileServiceDomainAppGroupDataContainer,
    // The temporary directory.
    Temporary             = IdeviceFileServiceDomainTemporary,
    // The system crash-log store.
    SystemCrashLogs       = IdeviceFileServiceDomainSystemCrashLogs,
};

// Looks a domain up by the name the device uses, e.g. "appDataContainer".
Option<FileServiceDomain> file_service_domain_from_name(const std::string& name);

// Browse and transfer files over the CoreDevice file service. Every command is
// scoped to the session opened with create_session().
class FileService {
  public:
    // The RSD service names this class connects: the control channel it speaks
    // on, and the data channel downloads are transferred over.
    static const char* CONTROL_SERVICE_NAME;
    static const char* DATA_SERVICE_NAME;

    // Factory: connect the control channel via RSD (borrows adapter & handshake)
    static Result<FileService, FfiError> connect_rsd(Adapter& adapter, RsdHandshake& rsd);

    // Factory: from socket Box<dyn ReadWrite> (consumes it).
    static Result<FileService, FfiError> from_readwrite_ptr(ReadWriteOpaque* consumed);

    // nice ergonomic overload: consume a C++ ReadWrite by releasing it
    static Result<FileService, FfiError> from_readwrite(ReadWrite&& rw);

    // Opens a session on `domain`, which every later command is scoped to, and
    // returns its ID. `identifier` names the container for the container
    // domains and is ignored by the others.
    Result<std::string, FfiError>
    create_session(FileServiceDomain domain, const std::string& identifier = std::string());

    // The session ID from the last create_session()
    Result<Option<std::string>, FfiError>    session_id() const;

    // Lists `path`, relative to the session's domain root
    Result<std::vector<std::string>, FfiError> list_directory(const std::string& path);

    // Downloads `path`, relative to the session's domain root. The transfer
    // runs on the data channel, which is opened off `adapter` at `data_port` —
    // the port the RSD handshake reports for DATA_SERVICE_NAME.
    Result<std::vector<uint8_t>, FfiError>
    retrieve_file(const std::string& path, Adapter& adapter, uint16_t data_port);

    // Same, over a data channel the caller already opened (consumed).
    Result<std::vector<uint8_t>, FfiError> retrieve_file(const std::string& path,
                                                         ReadWrite&&        data_stream);

    // Creates an empty file at `path`, relative to the session's domain root
    Result<void, FfiError>                 propose_empty_file(const std::string& path,
                                                              uint32_t           file_permissions = 0644,
                                                              uint32_t           uid              = 501,
                                                              uint32_t           gid              = 501,
                                                              int64_t            creation_time    = 0,
                                                              int64_t last_modification_time      = 0);

    // RAII / moves
    ~FileService() noexcept                          = default;
    FileService(FileService&&) noexcept              = default;
    FileService& operator=(FileService&&) noexcept   = default;
    FileService(const FileService&)                  = delete;
    FileService&       operator=(const FileService&) = delete;

    FileServiceHandle* raw() const noexcept { return handle_.get(); }
    static FileService adopt(FileServiceHandle* h) noexcept { return FileService(h); }

  private:
    explicit FileService(FileServiceHandle* h) noexcept : handle_(h) {}
    FileServicePtr handle_{};
};

} // namespace IdeviceFFI
