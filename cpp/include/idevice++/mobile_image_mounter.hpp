#pragma once
#include <cstdint>
#include <functional>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using MobileImageMounterPtr =
    std::unique_ptr<ImageMounterHandle, FnDeleter<ImageMounterHandle, image_mounter_free>>;

class MobileImageMounter {
  public:
    // Factory: connect via Provider
    static Result<MobileImageMounter, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<MobileImageMounter, FfiError> from_socket(Idevice&& socket);

    // Ops
    Result<std::vector<plist_t>, FfiError>      copy_devices();
    Result<std::vector<uint8_t>, FfiError>      lookup_image(std::string image_type);
    Result<void, FfiError>                      upload_image(std::string    image_type,
                                                             const uint8_t* image_data,
                                                             size_t         image_size,
                                                             const uint8_t* signature_data,
                                                             size_t         signature_size);
    Result<void, FfiError>                      mount_image(std::string    image_type,
                                                            const uint8_t* signature_data,
                                                            size_t         signature_size,
                                                            const uint8_t* trust_cache_data,
                                                            size_t         trust_cache_size,
                                                            plist_t        info_plist);
    Result<void, FfiError>                      unmount_image(std::string mount_path);
    Result<bool, FfiError>                      query_developer_mode_status();
    Result<void, FfiError>                      mount_developer(const uint8_t* image_data,
                                                                size_t         image_size,
                                                                const uint8_t* signature_data,
                                                                size_t         signature_size);
    Result<std::vector<uint8_t>, FfiError>      query_personalization_manifest(
             std::string image_type, const uint8_t* signature_data, size_t signature_size);
    Result<std::vector<uint8_t>, FfiError> query_nonce(std::string personalized_image_type);
    Result<plist_t, FfiError> query_personalization_identifiers(std::string image_type);
    Result<void, FfiError>    roll_personalization_nonce();
    Result<void, FfiError>    roll_cryptex_nonce();
    Result<void, FfiError>    mount_personalized(Provider&      provider,
                                                 const uint8_t* image_data,
                                                 size_t         image_size,
                                                 const uint8_t* trust_cache_data,
                                                 size_t         trust_cache_size,
                                                 const uint8_t* build_manifest_data,
                                                 size_t         build_manifest_size,
                                                 plist_t        info_plist,
                                                 uint64_t       unique_chip_id);
    Result<void, FfiError>
    mount_personalized_with_callback(Provider&                            provider,
                                     const uint8_t*                       image_data,
                                     size_t                               image_size,
                                     const uint8_t*                       trust_cache_data,
                                     size_t                               trust_cache_size,
                                     const uint8_t*                       build_manifest_data,
                                     size_t                               build_manifest_size,
                                     plist_t                              info_plist,
                                     uint64_t                             unique_chip_id,
                                     std::function<void(size_t, size_t)>& lambda);

    // RAII / moves
    ~MobileImageMounter() noexcept                                 = default;
    MobileImageMounter(MobileImageMounter&&) noexcept              = default;
    MobileImageMounter& operator=(MobileImageMounter&&) noexcept   = default;
    MobileImageMounter(const MobileImageMounter&)                  = delete;
    MobileImageMounter&       operator=(const MobileImageMounter&) = delete;

    ImageMounterHandle*       raw() const noexcept { return handle_.get(); }
    static MobileImageMounter adopt(ImageMounterHandle* h) noexcept {
        return MobileImageMounter(h);
    }

  private:
    explicit MobileImageMounter(ImageMounterHandle* h) noexcept : handle_(h) {}
    MobileImageMounterPtr handle_{};
};

} // namespace IdeviceFFI
