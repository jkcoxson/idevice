// Jackson Coxson

#include <cstring>
#include <idevice++/bindings.hpp>
#include <idevice++/mobilebackup2.hpp>
#include <vector>

namespace IdeviceFFI {

// -------- Trampoline Functions --------
// These bridge between the C function-pointer interface and C++ std::function.

namespace {

extern "C" uint64_t get_free_disk_space_trampoline(const char* path, void* ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    if (cbs.get_free_disk_space) {
        return cbs.get_free_disk_space(path ? path : "");
    }
    return 0;
}

static IdeviceFfiError* make_ffi_error(int32_t code, const char* msg) {
    auto* err    = static_cast<IdeviceFfiError*>(malloc(sizeof(IdeviceFfiError)));
    err->code    = code;
    err->message = strdup(msg);
    return err;
}

extern "C" IdeviceFfiError*
open_file_read_trampoline(const char* path, uint8_t** out_data, size_t* out_len, void* ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    *out_data = nullptr;
    *out_len  = 0;
    if (!cbs.open_file_read) {
        return make_ffi_error(-1, "no callback");
    }
    try {
        auto data = cbs.open_file_read(path ? path : "");
        *out_len  = data.size();
        if (!data.empty()) {
            *out_data = static_cast<uint8_t*>(malloc(data.size()));
            memcpy(*out_data, data.data(), data.size());
        }
        return nullptr;
    } catch (...) {
        return make_ffi_error(-1, "file not found");
    }
}

extern "C" IdeviceFfiError* create_file_write_trampoline(const char* path, void* ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    if (cbs.create_file_write) {
        try {
            cbs.create_file_write(path ? path : "");
        } catch (...) {
        }
    }
    return nullptr;
}

extern "C" IdeviceFfiError*
write_chunk_trampoline(const char* path, const uint8_t* data, size_t len, void* ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    if (cbs.write_chunk) {
        try {
            cbs.write_chunk(path ? path : "", data, len);
        } catch (...) {
        }
    }
    return nullptr;
}

extern "C" IdeviceFfiError* close_file_trampoline(const char* path, void* ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    if (cbs.close_file) {
        try {
            cbs.close_file(path ? path : "");
        } catch (...) {
        }
    }
    return nullptr;
}

extern "C" IdeviceFfiError* create_dir_all_trampoline(const char* path, void* ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    if (cbs.create_dir_all) {
        try {
            cbs.create_dir_all(path ? path : "");
        } catch (...) {
        }
    }
    return nullptr;
}

extern "C" IdeviceFfiError* remove_trampoline(const char* path, void* ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    if (cbs.remove) {
        try {
            cbs.remove(path ? path : "");
        } catch (...) {
        }
    }
    return nullptr;
}

extern "C" IdeviceFfiError* rename_trampoline(const char* from, const char* to, void* ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    if (cbs.rename) {
        try {
            cbs.rename(from ? from : "", to ? to : "");
        } catch (...) {
        }
    }
    return nullptr;
}

extern "C" IdeviceFfiError* copy_trampoline(const char* src, const char* dst, void* ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    if (cbs.copy) {
        try {
            cbs.copy(src ? src : "", dst ? dst : "");
        } catch (...) {
        }
    }
    return nullptr;
}

extern "C" bool exists_trampoline(const char* path, void* ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    if (cbs.exists) {
        return cbs.exists(path ? path : "");
    }
    return false;
}

extern "C" bool is_dir_trampoline(const char* path, void* ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    if (cbs.is_dir) {
        return cbs.is_dir(path ? path : "");
    }
    return false;
}

extern "C" void on_progress_trampoline(uint64_t bytes_done,
                                       uint64_t bytes_total,
                                       double   overall_progress,
                                       void*    ctx) {
    auto& cbs = *static_cast<BackupDelegateCallbacks*>(ctx);
    if (cbs.on_progress) {
        cbs.on_progress(bytes_done, bytes_total, overall_progress);
    }
}

Mobilebackup2BackupDelegateFFI make_ffi_delegate(BackupDelegateCallbacks& cbs) {
    Mobilebackup2BackupDelegateFFI d{};
    d.context             = &cbs;
    d.get_free_disk_space = get_free_disk_space_trampoline;
    d.open_file_read      = open_file_read_trampoline;
    d.create_file_write   = create_file_write_trampoline;
    d.write_chunk         = write_chunk_trampoline;
    d.close_file          = close_file_trampoline;
    d.create_dir_all      = create_dir_all_trampoline;
    d.remove              = remove_trampoline;
    d.rename              = rename_trampoline;
    d.copy                = copy_trampoline;
    d.exists              = exists_trampoline;
    d.is_dir              = is_dir_trampoline;
    d.on_progress         = cbs.on_progress ? on_progress_trampoline : nullptr;
    return d;
}

} // anonymous namespace

// -------- Factory Methods --------

Result<MobileBackup2, FfiError> MobileBackup2::connect(Provider& provider) {
    MobileBackup2ClientHandle* handle = nullptr;
    FfiError                   e(::mobilebackup2_connect(provider.raw(), &handle));
    if (e) {
        return Err(e);
    }
    return Ok(MobileBackup2::adopt(handle));
}

Result<MobileBackup2, FfiError> MobileBackup2::from_socket(Idevice&& socket) {
    MobileBackup2ClientHandle* handle = nullptr;
    FfiError                   e(::mobilebackup2_new(socket.release(), &handle));
    if (e) {
        return Err(e);
    }
    return Ok(MobileBackup2::adopt(handle));
}

// -------- Operations --------

Result<plist_t, FfiError> MobileBackup2::backup(const std::string&       backup_root,
                                                Option<std::string>      source_identifier,
                                                Option<plist_t>          options,
                                                BackupDelegateCallbacks& delegate) {
    auto        ffi_delegate = make_ffi_delegate(delegate);

    const char* source_ptr =
        source_identifier.is_some() ? source_identifier.unwrap().c_str() : nullptr;
    plist_t  opts_ptr = options.is_some() ? std::move(options).unwrap() : nullptr;
    plist_t  response = nullptr;

    FfiError e(::mobilebackup2_backup(
        this->raw(), backup_root.c_str(), source_ptr, opts_ptr, &ffi_delegate, &response));
    if (e) {
        return Err(e);
    }
    return Ok(response);
}

Result<plist_t, FfiError> MobileBackup2::restore(const std::string&       backup_root,
                                                 Option<std::string>      source_identifier,
                                                 Option<plist_t>          options,
                                                 BackupDelegateCallbacks& delegate) {
    auto        ffi_delegate = make_ffi_delegate(delegate);

    const char* source_ptr =
        source_identifier.is_some() ? source_identifier.unwrap().c_str() : nullptr;
    plist_t  opts_ptr = options.is_some() ? std::move(options).unwrap() : nullptr;
    plist_t  response = nullptr;

    FfiError e(::mobilebackup2_restore(
        this->raw(), backup_root.c_str(), source_ptr, opts_ptr, &ffi_delegate, &response));
    if (e) {
        return Err(e);
    }
    return Ok(response);
}

Result<void, FfiError> MobileBackup2::change_password(const std::string&       backup_root,
                                                      Option<std::string>      old_password,
                                                      Option<std::string>      new_password,
                                                      BackupDelegateCallbacks& delegate) {
    auto        ffi_delegate = make_ffi_delegate(delegate);

    const char* old_ptr      = old_password.is_some() ? old_password.unwrap().c_str() : nullptr;
    const char* new_ptr      = new_password.is_some() ? new_password.unwrap().c_str() : nullptr;

    FfiError    e(::mobilebackup2_change_password(
        this->raw(), backup_root.c_str(), old_ptr, new_ptr, &ffi_delegate));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> MobileBackup2::disconnect() {
    FfiError e(::mobilebackup2_disconnect(this->raw()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
