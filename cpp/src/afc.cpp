// Jackson Coxson

#include <cstdlib>
#include <idevice++/afc.hpp>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

// -------- AfcClient Factory Methods --------

Result<AfcClient, FfiError> AfcClient::connect(Provider& provider) {
    AfcClientHandle* out = nullptr;
    FfiError         e(::afc_client_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(AfcClient::adopt(out));
}

Result<AfcClient, FfiError> AfcClient::connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake) {
    AfcClientHandle* out = nullptr;
    FfiError         e(::afc_client_connect_rsd(adapter, handshake, &out));
    if (e) {
        return Err(e);
    }
    return Ok(AfcClient::adopt(out));
}

Result<AfcClient, FfiError> AfcClient::connect_afc2(Provider& provider) {
    AfcClientHandle* out = nullptr;
    FfiError         e(::afc2_client_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(AfcClient::adopt(out));
}

Result<AfcClient, FfiError> AfcClient::from_socket(Idevice&& socket) {
    AfcClientHandle* out = nullptr;
    FfiError         e(::afc_client_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(AfcClient::adopt(out));
}

// -------- AfcClient Ops --------

Result<std::vector<std::string>, FfiError> AfcClient::list_directory(const std::string& path) {
    char**   entries_raw = nullptr;
    size_t   count       = 0;

    FfiError e(::afc_list_directory(handle_.get(), path.c_str(), &entries_raw, &count));
    if (e) {
        return Err(e);
    }

    std::vector<std::string> result;
    if (entries_raw) {
        result.reserve(count);
        for (size_t i = 0; i < count; ++i) {
            if (entries_raw[i]) {
                result.emplace_back(entries_raw[i]);
                ::idevice_string_free(entries_raw[i]);
            }
        }
        std::free(entries_raw);
    }

    return Ok(std::move(result));
}

Result<void, FfiError> AfcClient::make_directory(const std::string& path) {
    FfiError e(::afc_make_directory(handle_.get(), path.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<AfcFileInfo, FfiError> AfcClient::get_file_info(const std::string& path) {
    AfcFileInfo info{};
    FfiError    e(::afc_get_file_info(handle_.get(), path.c_str(), &info));
    if (e) {
        return Err(e);
    }
    // Copy out the info and free the FFI-allocated strings
    AfcFileInfo result = info;
    // Note: caller should use afc_file_info_free when done, but since we're
    // returning a copy, we need to manage the strings ourselves.
    // We leave the raw struct as-is since the strings are owned by the FFI.
    return Ok(std::move(result));
}

Result<AfcDeviceInfo, FfiError> AfcClient::get_device_info() {
    AfcDeviceInfo info{};
    FfiError      e(::afc_get_device_info(handle_.get(), &info));
    if (e) {
        return Err(e);
    }
    return Ok(std::move(info));
}

Result<void, FfiError> AfcClient::remove_path(const std::string& path) {
    FfiError e(::afc_remove_path(handle_.get(), path.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> AfcClient::remove_path_and_contents(const std::string& path) {
    FfiError e(::afc_remove_path_and_contents(handle_.get(), path.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<AfcFile, FfiError> AfcClient::file_open(const std::string& path, AfcFopenMode mode) {
    AfcFileHandle* out = nullptr;
    FfiError       e(::afc_file_open(handle_.get(), path.c_str(), mode, &out));
    if (e) {
        return Err(e);
    }
    return Ok(AfcFile::adopt(out));
}

Result<void, FfiError>
AfcClient::make_link(const std::string& target, const std::string& source, AfcLinkType link_type) {
    FfiError e(::afc_make_link(handle_.get(), target.c_str(), source.c_str(), link_type));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> AfcClient::rename_path(const std::string& source,
                                              const std::string& target) {
    FfiError e(::afc_rename_path(handle_.get(), source.c_str(), target.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

// -------- AfcFile --------

AfcFile::~AfcFile() noexcept {
    if (handle_) {
        ::afc_file_close(handle_);
        handle_ = nullptr;
    }
}

AfcFile::AfcFile(AfcFile&& o) noexcept : handle_(o.handle_) {
    o.handle_ = nullptr;
}

AfcFile& AfcFile::operator=(AfcFile&& o) noexcept {
    if (this != &o) {
        if (handle_) {
            ::afc_file_close(handle_);
        }
        handle_   = o.handle_;
        o.handle_ = nullptr;
    }
    return *this;
}

Result<void, FfiError> AfcFile::close() {
    if (!handle_) {
        return Ok();
    }
    FfiError e(::afc_file_close(handle_));
    handle_ = nullptr;
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<std::vector<uint8_t>, FfiError> AfcFile::read(size_t len) {
    uint8_t* data       = nullptr;
    size_t   bytes_read = 0;
    FfiError e(::afc_file_read(handle_, &data, len, &bytes_read));
    if (e) {
        return Err(e);
    }
    std::vector<uint8_t> result;
    if (data && bytes_read > 0) {
        result.assign(data, data + bytes_read);
        ::afc_file_read_data_free(data, bytes_read);
    }
    return Ok(std::move(result));
}

Result<std::vector<uint8_t>, FfiError> AfcFile::read_all() {
    uint8_t* data   = nullptr;
    size_t   length = 0;
    FfiError e(::afc_file_read_entire(handle_, &data, &length));
    if (e) {
        return Err(e);
    }
    std::vector<uint8_t> result;
    if (data && length > 0) {
        result.assign(data, data + length);
        ::afc_file_read_data_free(data, length);
    }
    return Ok(std::move(result));
}

Result<int64_t, FfiError> AfcFile::seek(int64_t offset, int whence) {
    int64_t  new_pos = 0;
    FfiError e(::afc_file_seek(handle_, offset, whence, &new_pos));
    if (e) {
        return Err(e);
    }
    return Ok(new_pos);
}

Result<int64_t, FfiError> AfcFile::tell() {
    int64_t  pos = 0;
    FfiError e(::afc_file_tell(handle_, &pos));
    if (e) {
        return Err(e);
    }
    return Ok(pos);
}

Result<void, FfiError> AfcFile::write(const uint8_t* data, size_t length) {
    FfiError e(::afc_file_write(handle_, data, length));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
