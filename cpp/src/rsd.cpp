// Jackson Coxson

#include <idevice++/rsd.hpp>

namespace IdeviceFFI {

// ---------- helpers to copy/free CRsdService ----------
static RsdService to_cpp_and_free(CRsdService* c) {
    RsdService s;
    if (c->name) {
        s.name = c->name;
    }
    if (c->entitlement) {
        s.entitlement = c->entitlement;
    }
    s.port            = c->port;
    s.uses_remote_xpc = c->uses_remote_xpc;
    s.service_version = c->service_version;

    // features
    if (c->features && c->features_count > 0) {
        auto** arr = c->features;
        s.features.reserve(c->features_count);
        for (size_t i = 0; i < c->features_count; ++i) {
            if (arr[i]) {
                s.features.emplace_back(arr[i]);
            }
        }
    }

    // release the C allocation now that we've copied
    rsd_free_service(c);
    return s;
}

static std::vector<RsdService> to_cpp_and_free(CRsdServiceArray* arr) {
    std::vector<RsdService> out;
    if (!arr || !arr->services || arr->count == 0) {
        if (arr) {
            rsd_free_services(arr);
        }
        return out;
    }
    out.reserve(arr->count);
    auto* begin = arr->services;
    for (size_t i = 0; i < arr->count; ++i) {
        out.emplace_back(RsdService{begin[i].name ? begin[i].name : "",
                                    begin[i].entitlement ? begin[i].entitlement : "",
                                    begin[i].port,
                                    begin[i].uses_remote_xpc,
                                    {}, // features, fill below
                                    begin[i].service_version});
        // features for this service
        if (begin[i].features && begin[i].features_count > 0) {
            auto** feats = begin[i].features;
            out.back().features.reserve(begin[i].features_count);
            for (size_t j = 0; j < begin[i].features_count; ++j) {
                if (feats[j]) {
                    out.back().features.emplace_back(feats[j]);
                }
            }
        }
    }
    // free the array + nested C strings now that we've copied
    rsd_free_services(arr);
    return out;
}

RsdHandshake::RsdHandshake(const RsdHandshake& other) {
    if (other.handle_) {
        // Call the Rust FFI to clone the underlying handle
        handle_.reset(rsd_handshake_clone(other.handle_.get()));
    }
    // If other.handle_ is null, our new handle_ will also be null, which is correct.
}

RsdHandshake& RsdHandshake::operator=(const RsdHandshake& other) {
    // Check for self-assignment
    if (this != &other) {
        // Create a temporary copy, then swap ownership
        RsdHandshake temp(other);
        std::swap(handle_, temp.handle_);
    }
    return *this;
}

// ---------- factory ----------
Result<RsdHandshake, FfiError> RsdHandshake::from_socket(ReadWrite&& rw) {
    RsdHandshakeHandle* out = nullptr;

    // Rust consumes the socket regardless of result ⇒ release BEFORE call.
    ReadWriteOpaque*    raw = rw.release();
    FfiError            e(rsd_handshake_new(raw, &out));
    if (e) {
        return Err(e);
    }
    return Ok(RsdHandshake::adopt(out));
}

// ---------- queries ----------
Result<size_t, FfiError> RsdHandshake::protocol_version() const {
    size_t   v = 0;
    FfiError e(rsd_get_protocol_version(handle_.get(), &v));
    if (e) {
        return Err(e);
    }
    return Ok(v);
}

Result<std::string, FfiError> RsdHandshake::uuid() const {
    char*    c = nullptr;
    FfiError e(rsd_get_uuid(handle_.get(), &c));
    if (e) {
        return Err(e);
    }
    std::string out;
    if (c) {
        out = c;
        rsd_free_string(c);
    }
    return Ok(out);
}

Result<std::vector<RsdService>, FfiError> RsdHandshake::services() const {
    CRsdServiceArray* arr = nullptr;
    FfiError          e(rsd_get_services(handle_.get(), &arr));
    if (e) {
        return Err(e);
    }
    return Ok(to_cpp_and_free(arr));
}

Result<bool, FfiError> RsdHandshake::service_available(const std::string& name) const {
    bool     avail = false;
    FfiError e(rsd_service_available(handle_.get(), name.c_str(), &avail));
    if (e) {
        return Err(e);
    }
    return Ok(avail);
}

Result<RsdService, FfiError> RsdHandshake::service_info(const std::string& name) const {
    CRsdService* svc = nullptr;
    FfiError     e(rsd_get_service_info(handle_.get(), name.c_str(), &svc));
    if (e) {
        return Err(e);
    }
    return Ok(to_cpp_and_free(svc));
}

} // namespace IdeviceFFI
