// Jackson Coxson

#include <idevice++/cryptexd.hpp>

namespace IdeviceFFI {

// ---- Helpers ----
static ::CryptexNonceDomain to_c(NonceDomain domain) {
    ::CryptexNonceDomain c;
    c.is_handle = domain.is_handle ? 1 : 0;
    c.value     = domain.value;
    return c;
}

static InstalledCryptex take_installed(InstalledCryptexC& c) {
    InstalledCryptex out;
    if (c.identifier) {
        out.identifier = c.identifier;
    }
    if (c.version) {
        out.version = c.version;
    }
    return out;
}

static std::vector<uint8_t> take_data(uint8_t* data, size_t len) {
    std::vector<uint8_t> out;
    if (data && len) {
        out.assign(data, data + len);
    }
    ::idevice_data_free(data, len);
    return out;
}

// ---- Cryptex1Assets ----
Result<Cryptex1Assets, FfiError> Cryptex1Assets::load(const std::string& restore_dir) {
    Cryptex1AssetsHandle* out = nullptr;
    if (IdeviceFfiError* e = ::cryptex1_assets_load(restore_dir.c_str(), &out)) {
        return Err(FfiError(e));
    }
    return Ok(Cryptex1Assets::adopt(out));
}

Result<Cryptex1Assets, FfiError> Cryptex1Assets::from_parts(const std::vector<uint8_t>& image,
                                                            const std::vector<uint8_t>& trustcache,
                                                            const std::vector<uint8_t>& info,
                                                            const std::vector<uint8_t>& volumehash,
                                                            plist_t build_identity) {
    Cryptex1AssetsHandle* out = nullptr;
    if (IdeviceFfiError* e = ::cryptex1_assets_from_parts(image.data(),
                                                          image.size(),
                                                          trustcache.data(),
                                                          trustcache.size(),
                                                          info.data(),
                                                          info.size(),
                                                          volumehash.data(),
                                                          volumehash.size(),
                                                          build_identity,
                                                          &out)) {
        return Err(FfiError(e));
    }
    return Ok(Cryptex1Assets::adopt(out));
}

Result<uint64_t, FfiError> Cryptex1Assets::nonce_domain() const {
    uint64_t domain = 0;
    if (IdeviceFfiError* e = ::cryptex1_assets_nonce_domain(handle_.get(), &domain)) {
        return Err(FfiError(e));
    }
    return Ok(domain);
}

// ---- Factories ----
Result<Cryptexd, FfiError> Cryptexd::connect_rsd(Adapter& adapter, RsdHandshake& rsd) {
    CryptexdHandle* out = nullptr;
    if (IdeviceFfiError* e = ::cryptexd_connect_rsd(adapter.raw(), rsd.raw(), &out)) {
        return Err(FfiError(e));
    }
    return Ok(Cryptexd::adopt(out));
}

Result<Cryptexd, FfiError> Cryptexd::from_readwrite_ptr(ReadWriteOpaque* consumed) {
    CryptexdHandle* out = nullptr;
    if (IdeviceFfiError* e = ::cryptexd_new(consumed, &out)) {
        return Err(FfiError(e));
    }
    return Ok(Cryptexd::adopt(out));
}

Result<Cryptexd, FfiError> Cryptexd::from_readwrite(ReadWrite&& rw) {
    // Rust consumes the stream regardless of result → release BEFORE call
    return from_readwrite_ptr(rw.release());
}

// ---- Routines ----
// Each of these consumes the client handle, so release it before the call.
Result<plist_t, FfiError> Cryptexd::read_personalization_identifiers() {
    plist_t out = nullptr;
    if (IdeviceFfiError* e =
            ::cryptexd_read_personalization_identifiers(handle_.release(), &out)) {
        return Err(FfiError(e));
    }
    return Ok(out);
}

Result<std::vector<InstalledCryptex>, FfiError> Cryptexd::copy_installed() {
    InstalledCryptexC* arr = nullptr;
    size_t             n   = 0;
    if (IdeviceFfiError* e = ::cryptexd_copy_installed(handle_.release(), &arr, &n)) {
        return Err(FfiError(e));
    }

    std::vector<InstalledCryptex> out;
    out.reserve(n);
    for (size_t i = 0; i < n; ++i) {
        out.emplace_back(take_installed(arr[i]));
    }
    ::cryptexd_free_installed(arr, n);
    return Ok(std::move(out));
}

Result<std::vector<uint8_t>, FfiError> Cryptexd::get_nonce(NonceDomain domain) {
    uint8_t* data = nullptr;
    size_t   n    = 0;
    if (IdeviceFfiError* e = ::cryptexd_get_nonce(handle_.release(), to_c(domain), &data, &n)) {
        return Err(FfiError(e));
    }
    return Ok(take_data(data, n));
}

Result<std::vector<uint8_t>, FfiError> Cryptexd::cryptex_nonce(uint64_t nonce_domain_handle) {
    uint8_t* data = nullptr;
    size_t   n    = 0;
    if (IdeviceFfiError* e =
            ::cryptexd_cryptex_nonce(handle_.release(), nonce_domain_handle, &data, &n)) {
        return Err(FfiError(e));
    }
    return Ok(take_data(data, n));
}

Result<void, FfiError> Cryptexd::roll_nonce(NonceDomain domain) {
    if (IdeviceFfiError* e = ::cryptexd_roll_nonce(handle_.release(), to_c(domain))) {
        return Err(FfiError(e));
    }
    return Ok();
}

Result<void, FfiError> Cryptexd::uninstall(const std::string&         identifier,
                                           const Option<std::string>& version) {
    if (IdeviceFfiError* e =
            ::cryptexd_uninstall(handle_.release(),
                                 identifier.c_str(),
                                 version.is_some() ? version.unwrap().c_str() : nullptr)) {
        return Err(FfiError(e));
    }
    return Ok();
}

Result<void, FfiError> Cryptexd::install(const CryptexInstallRequest& request) {
    ::CryptexInstallRequestC c;
    c.image               = request.image.data();
    c.image_len           = request.image.size();
    c.trustcache          = request.trustcache.data();
    c.trustcache_len      = request.trustcache.size();
    c.im4m                = request.im4m.data();
    c.im4m_len            = request.im4m.size();
    c.info                = request.info.data();
    c.info_len            = request.info.size();
    c.volumehash          = request.volumehash.data();
    c.volumehash_len      = request.volumehash.size();
    c.cryptex1_properties = request.cryptex1_properties;
    c.image_type_index    = request.image_type_index;
    c.persistence         = request.persistence;
    c.nonce_persistence   = request.nonce_persistence;
    c.auth                = request.auth;

    if (IdeviceFfiError* e = ::cryptexd_install(handle_.release(), &c)) {
        return Err(FfiError(e));
    }
    return Ok();
}

// ---- Free functions ----
Result<std::vector<uint8_t>, FfiError> cryptexd_unwrap_nonce(const std::vector<uint8_t>& blob) {
    uint8_t* data = nullptr;
    size_t   n    = 0;
    if (IdeviceFfiError* e = ::cryptexd_unwrap_nonce(blob.data(), blob.size(), &data, &n)) {
        return Err(FfiError(e));
    }
    return Ok(take_data(data, n));
}

Result<InstalledCryptex, FfiError>
cryptexd_install_ddi(Adapter& adapter, RsdHandshake& rsd, Cryptex1Assets& assets) {
    InstalledCryptexC* installed = nullptr;
    if (IdeviceFfiError* e =
            ::cryptexd_install_ddi(adapter.raw(), rsd.raw(), assets.raw(), &installed)) {
        return Err(FfiError(e));
    }

    InstalledCryptex out;
    if (installed) {
        out = take_installed(*installed);
        ::cryptexd_free_installed_cryptex(installed);
    }
    return Ok(std::move(out));
}

Result<Option<InstalledCryptex>, FfiError> cryptexd_installed_ddi(Adapter&      adapter,
                                                                  RsdHandshake& rsd) {
    InstalledCryptexC* installed = nullptr;
    if (IdeviceFfiError* e = ::cryptexd_installed_ddi(adapter.raw(), rsd.raw(), &installed)) {
        return Err(FfiError(e));
    }

    Option<InstalledCryptex> out;
    if (installed) {
        out = take_installed(*installed);
        ::cryptexd_free_installed_cryptex(installed);
    }
    return Ok(std::move(out));
}

} // namespace IdeviceFFI
