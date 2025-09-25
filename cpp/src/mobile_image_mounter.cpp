// Jackson Coxson

#include <idevice++/mobile_image_mounter.hpp>
#include <vector>

namespace IdeviceFFI {

// -------- Anonymous Namespace for Helpers --------
namespace {
/**
 * @brief A C-style trampoline function to call back into a C++ std::function.
 *
 * This function is passed to the Rust FFI layer. It receives a void* context,
 * which it casts back to the original std::function object to invoke it.
 */
extern "C" void progress_trampoline(size_t progress, size_t total, void* context) {
    if (context) {
        auto& callback_fn = *static_cast<std::function<void(size_t, size_t)>*>(context);
        callback_fn(progress, total);
    }
}
} // namespace

// -------- Factory Methods --------

Result<MobileImageMounter, FfiError> MobileImageMounter::connect(Provider& provider) {
    ImageMounterHandle* handle = nullptr;
    FfiError            e(::image_mounter_connect(provider.raw(), &handle));
    if (e) {
        return Err(e);
    }
    return Ok(MobileImageMounter::adopt(handle));
}

Result<MobileImageMounter, FfiError> MobileImageMounter::from_socket(Idevice&& socket) {
    ImageMounterHandle* handle = nullptr;
    // The Rust FFI function consumes the socket, so we must release it from the
    // C++ RAII wrapper's control. An `Idevice::release()` method is assumed here.
    FfiError            e(::image_mounter_new(socket.release(), &handle));
    if (e) {
        return Err(e);
    }
    return Ok(MobileImageMounter::adopt(handle));
}

// -------- Ops --------

Result<std::vector<plist_t>, FfiError> MobileImageMounter::copy_devices() {
    plist_t* devices_raw = nullptr;
    size_t   devices_len = 0;

    FfiError e(::image_mounter_copy_devices(this->raw(), &devices_raw, &devices_len));
    if (e) {
        return Err(e);
    }

    std::vector<plist_t> devices;
    if (devices_raw) {
        devices.assign(devices_raw, devices_raw + devices_len);
    }

    return Ok(std::move(devices));
}

Result<std::vector<uint8_t>, FfiError> MobileImageMounter::lookup_image(std::string image_type) {
    uint8_t* signature_raw = nullptr;
    size_t   signature_len = 0;

    FfiError e(::image_mounter_lookup_image(
        this->raw(), image_type.c_str(), &signature_raw, &signature_len));
    if (e) {
        return Err(e);
    }

    std::vector<uint8_t> signature(signature_len);
    std::memcpy(signature.data(), signature_raw, signature_len);
    idevice_data_free(signature_raw, signature_len);

    return Ok(std::move(signature));
}

Result<void, FfiError> MobileImageMounter::upload_image(std::string    image_type,
                                                        const uint8_t* image_data,
                                                        size_t         image_size,
                                                        const uint8_t* signature_data,
                                                        size_t         signature_size) {
    FfiError e(::image_mounter_upload_image(
        this->raw(), image_type.c_str(), image_data, image_size, signature_data, signature_size));
    return e ? Result<void, FfiError>(Err(e)) : Result<void, FfiError>(Ok());
}

Result<void, FfiError> MobileImageMounter::mount_image(std::string    image_type,
                                                       const uint8_t* signature_data,
                                                       size_t         signature_size,
                                                       const uint8_t* trust_cache_data,
                                                       size_t         trust_cache_size,
                                                       plist_t        info_plist) {
    FfiError e(::image_mounter_mount_image(this->raw(),
                                           image_type.c_str(),
                                           signature_data,
                                           signature_size,
                                           trust_cache_data,
                                           trust_cache_size,
                                           info_plist));
    return e ? Result<void, FfiError>(Err(e)) : Result<void, FfiError>(Ok());
}

Result<void, FfiError> MobileImageMounter::unmount_image(std::string mount_path) {
    FfiError e(::image_mounter_unmount_image(this->raw(), mount_path.c_str()));
    return e ? Result<void, FfiError>(Err(e)) : Result<void, FfiError>(Ok());
}

Result<bool, FfiError> MobileImageMounter::query_developer_mode_status() {
    int      status_c = 0;
    FfiError e(::image_mounter_query_developer_mode_status(this->raw(), &status_c));
    if (e) {
        return Err(e);
    }
    return Ok(status_c != 0);
}

Result<void, FfiError> MobileImageMounter::mount_developer(const uint8_t* image_data,
                                                           size_t         image_size,
                                                           const uint8_t* signature_data,
                                                           size_t         signature_size) {
    FfiError e(::image_mounter_mount_developer(
        this->raw(), image_data, image_size, signature_data, signature_size));
    return e ? Result<void, FfiError>(Err(e)) : Result<void, FfiError>(Ok());
}

Result<std::vector<uint8_t>, FfiError> MobileImageMounter::query_personalization_manifest(
    std::string image_type, const uint8_t* signature_data, size_t signature_size) {
    uint8_t* manifest_raw = nullptr;
    size_t   manifest_len = 0;
    FfiError e(::image_mounter_query_personalization_manifest(this->raw(),
                                                              image_type.c_str(),
                                                              signature_data,
                                                              signature_size,
                                                              &manifest_raw,
                                                              &manifest_len));
    if (e) {
        return Err(e);
    }

    std::vector<uint8_t> manifest(manifest_len);
    std::memcpy(manifest.data(), manifest_raw, manifest_len);
    idevice_data_free(manifest_raw, manifest_len);

    return Ok(std::move(manifest));
}

Result<std::vector<uint8_t>, FfiError>
MobileImageMounter::query_nonce(std::string personalized_image_type) {
    uint8_t*    nonce_raw = nullptr;
    size_t      nonce_len = 0;
    const char* image_type_c =
        personalized_image_type.empty() ? nullptr : personalized_image_type.c_str();

    FfiError e(::image_mounter_query_nonce(this->raw(), image_type_c, &nonce_raw, &nonce_len));
    if (e) {
        return Err(e);
    }

    std::vector<uint8_t> nonce(nonce_len);
    std::memcpy(nonce.data(), nonce_raw, nonce_len);
    idevice_data_free(nonce_raw, nonce_len);

    return Ok(std::move(nonce));
}

Result<plist_t, FfiError>
MobileImageMounter::query_personalization_identifiers(std::string image_type) {
    plist_t     identifiers  = nullptr;
    const char* image_type_c = image_type.empty() ? nullptr : image_type.c_str();

    FfiError    e(
        ::image_mounter_query_personalization_identifiers(this->raw(), image_type_c, &identifiers));
    if (e) {
        return Err(e);
    }

    // The caller now owns the returned `plist_t` and is responsible for freeing it.
    return Ok(identifiers);
}

Result<void, FfiError> MobileImageMounter::roll_personalization_nonce() {
    FfiError e(::image_mounter_roll_personalization_nonce(this->raw()));
    return e ? Result<void, FfiError>(Err(e)) : Result<void, FfiError>(Ok());
}

Result<void, FfiError> MobileImageMounter::roll_cryptex_nonce() {
    FfiError e(::image_mounter_roll_cryptex_nonce(this->raw()));
    return e ? Result<void, FfiError>(Err(e)) : Result<void, FfiError>(Ok());
}

Result<void, FfiError> MobileImageMounter::mount_personalized(Provider&      provider,
                                                              const uint8_t* image_data,
                                                              size_t         image_size,
                                                              const uint8_t* trust_cache_data,
                                                              size_t         trust_cache_size,
                                                              const uint8_t* build_manifest_data,
                                                              size_t         build_manifest_size,
                                                              plist_t        info_plist,
                                                              uint64_t       unique_chip_id) {
    FfiError e(::image_mounter_mount_personalized(this->raw(),
                                                  provider.raw(),
                                                  image_data,
                                                  image_size,
                                                  trust_cache_data,
                                                  trust_cache_size,
                                                  build_manifest_data,
                                                  build_manifest_size,
                                                  info_plist,
                                                  unique_chip_id));
    return e ? Result<void, FfiError>(Err(e)) : Result<void, FfiError>(Ok());
}

Result<void, FfiError>
MobileImageMounter::mount_personalized_with_callback(Provider&      provider,
                                                     const uint8_t* image_data,
                                                     size_t         image_size,
                                                     const uint8_t* trust_cache_data,
                                                     size_t         trust_cache_size,
                                                     const uint8_t* build_manifest_data,
                                                     size_t         build_manifest_size,
                                                     plist_t        info_plist,
                                                     uint64_t       unique_chip_id,
                                                     std::function<void(size_t, size_t)>& lambda) {

    FfiError e(::image_mounter_mount_personalized_with_callback(this->raw(),
                                                                provider.raw(),
                                                                image_data,
                                                                image_size,
                                                                trust_cache_data,
                                                                trust_cache_size,
                                                                build_manifest_data,
                                                                build_manifest_size,
                                                                info_plist,
                                                                unique_chip_id,
                                                                progress_trampoline,
                                                                &lambda /* context */));

    return e ? Result<void, FfiError>(Err(e)) : Result<void, FfiError>(Ok());
}

} // namespace IdeviceFFI
