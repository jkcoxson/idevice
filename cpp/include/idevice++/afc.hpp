// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using AfcClientPtr = std::unique_ptr<AfcClientHandle, FnDeleter<AfcClientHandle, afc_client_free>>;

class AfcFile;

class AfcClient {
  public:
    // Factory: connect via Provider (AFC)
    static Result<AfcClient, FfiError>         connect(Provider& provider);

    // Factory: connect via Provider (AFC2)
    static Result<AfcClient, FfiError>         connect_afc2(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<AfcClient, FfiError>         from_socket(Idevice&& socket);

    // Ops
    Result<std::vector<std::string>, FfiError> list_directory(const std::string& path);
    Result<void, FfiError>                     make_directory(const std::string& path);
    Result<AfcFileInfo, FfiError>              get_file_info(const std::string& path);
    Result<AfcDeviceInfo, FfiError>            get_device_info();
    Result<void, FfiError>                     remove_path(const std::string& path);
    Result<void, FfiError>                     remove_path_and_contents(const std::string& path);
    Result<AfcFile, FfiError> file_open(const std::string& path, AfcFopenMode mode);
    Result<void, FfiError>
    make_link(const std::string& target, const std::string& source, AfcLinkType link_type);
    Result<void, FfiError> rename_path(const std::string& source, const std::string& target);

    // RAII / moves
    ~AfcClient() noexcept                        = default;
    AfcClient(AfcClient&&) noexcept              = default;
    AfcClient& operator=(AfcClient&&) noexcept   = default;
    AfcClient(const AfcClient&)                  = delete;
    AfcClient&       operator=(const AfcClient&) = delete;

    AfcClientHandle* raw() const noexcept { return handle_.get(); }
    static AfcClient adopt(AfcClientHandle* h) noexcept { return AfcClient(h); }

  private:
    explicit AfcClient(AfcClientHandle* h) noexcept : handle_(h) {}
    AfcClientPtr handle_{};
};

class AfcFile {
  public:
    // Ops
    Result<void, FfiError>                 close();
    Result<std::vector<uint8_t>, FfiError> read(size_t len);
    Result<std::vector<uint8_t>, FfiError> read_all();
    Result<int64_t, FfiError>              seek(int64_t offset, int whence);
    Result<int64_t, FfiError>              tell();
    Result<void, FfiError>                 write(const uint8_t* data, size_t length);

    // RAII / moves
    ~AfcFile() noexcept;
    AfcFile(AfcFile&&) noexcept;
    AfcFile& operator=(AfcFile&&) noexcept;
    AfcFile(const AfcFile&)                  = delete;
    AfcFile&       operator=(const AfcFile&) = delete;

    AfcFileHandle* raw() const noexcept { return handle_; }
    static AfcFile adopt(AfcFileHandle* h) noexcept { return AfcFile(h); }

  private:
    explicit AfcFile(AfcFileHandle* h) noexcept : handle_(h) {}
    AfcFileHandle* handle_ = nullptr;
};

} // namespace IdeviceFFI
