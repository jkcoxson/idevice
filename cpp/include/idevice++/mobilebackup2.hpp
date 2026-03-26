// Jackson Coxson

#pragma once
#include <functional>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using MobileBackup2Ptr =
    std::unique_ptr<MobileBackup2ClientHandle,
                    FnDeleter<MobileBackup2ClientHandle, mobilebackup2_client_free>>;

/// Callback types for BackupDelegate operations.
/// The C++ caller provides these to drive filesystem I/O during backup/restore.
struct BackupDelegateCallbacks {
    /// Return available disk space in bytes for the given path
    std::function<uint64_t(const std::string&)>                     get_free_disk_space;

    /// Read an entire file and return its contents
    std::function<std::vector<uint8_t>(const std::string&)>         open_file_read;

    /// Create/truncate a file for writing (called before write_chunk)
    std::function<void(const std::string&)>                         create_file_write;

    /// Write a chunk of data to the currently open file
    std::function<void(const std::string&, const uint8_t*, size_t)> write_chunk;

    /// Close the current file (called after all write_chunk calls)
    std::function<void(const std::string&)>                         close_file;

    /// Recursively create a directory and all parents
    std::function<void(const std::string&)>                         create_dir_all;

    /// Remove a file or directory recursively
    std::function<void(const std::string&)>                         remove;

    /// Rename/move a file or directory
    std::function<void(const std::string&, const std::string&)>     rename;

    /// Copy a file or directory
    std::function<void(const std::string&, const std::string&)>     copy;

    /// Check if a path exists
    std::function<bool(const std::string&)>                         exists;

    /// Check if a path is a directory
    std::function<bool(const std::string&)>                         is_dir;

    /// Optional progress callback: bytes_done, bytes_total, overall_progress
    std::function<void(uint64_t, uint64_t, double)>                 on_progress;
};

class MobileBackup2 {
  public:
    // Factory: connect via Provider
    static Result<MobileBackup2, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<MobileBackup2, FfiError> from_socket(Idevice&& socket);

    // Operations
    Result<plist_t, FfiError>              backup(const std::string&       backup_root,
                                                  Option<std::string>      source_identifier,
                                                  Option<plist_t>          options,
                                                  BackupDelegateCallbacks& delegate);

    Result<plist_t, FfiError>              restore(const std::string&       backup_root,
                                                   Option<std::string>      source_identifier,
                                                   Option<plist_t>          options,
                                                   BackupDelegateCallbacks& delegate);

    Result<void, FfiError>                 change_password(const std::string&       backup_root,
                                                           Option<std::string>      old_password,
                                                           Option<std::string>      new_password,
                                                           BackupDelegateCallbacks& delegate);

    Result<void, FfiError>                 disconnect();

    // RAII / moves
    ~MobileBackup2() noexcept                                  = default;
    MobileBackup2(MobileBackup2&&) noexcept                    = default;
    MobileBackup2& operator=(MobileBackup2&&) noexcept         = default;
    MobileBackup2(const MobileBackup2&)                        = delete;
    MobileBackup2&             operator=(const MobileBackup2&) = delete;

    MobileBackup2ClientHandle* raw() const noexcept { return handle_.get(); }
    static MobileBackup2 adopt(MobileBackup2ClientHandle* h) noexcept { return MobileBackup2(h); }

  private:
    explicit MobileBackup2(MobileBackup2ClientHandle* h) noexcept : handle_(h) {}
    MobileBackup2Ptr handle_{};
};

} // namespace IdeviceFFI
