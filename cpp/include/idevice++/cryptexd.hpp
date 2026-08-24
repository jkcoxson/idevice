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

using CryptexdPtr = std::unique_ptr<CryptexdHandle, FnDeleter<CryptexdHandle, cryptexd_free>>;

using Cryptex1AssetsPtr =
    std::unique_ptr<Cryptex1AssetsHandle, FnDeleter<Cryptex1AssetsHandle, cryptex1_assets_free>>;

// A cryptex currently installed on the device, e.g. the mounted
// DeveloperDiskImage.
struct InstalledCryptex {
    std::string identifier;
    std::string version;
};

// Which nonce domain a get_nonce()/roll_nonce() refers to: either a domain
// index or a domain handle, e.g. a build identity's `Cryptex1,NonceDomain`.
struct NonceDomain {
    bool     is_handle{};
    uint64_t value{};

    static NonceDomain index(uint64_t index) {
        NonceDomain d;
        d.is_handle = false;
        d.value     = index;
        return d;
    }
    static NonceDomain handle(uint64_t handle) {
        NonceDomain d;
        d.is_handle = true;
        d.value     = handle;
        return d;
    }
    // The domain cryptexes are personalized against.
    static NonceDomain cryptex() { return index(IDEVICE_CRYPTEXD_NONCE_DOMAIN_CRYPTEX); }
};

// The payloads and parameters one install() needs.
struct CryptexInstallRequest {
    // The cryptex disk image, i.e. the manifest's `Cryptex1,GenericDmg`
    std::vector<uint8_t> image;
    // `Cryptex1,GenericTrustCache`
    std::vector<uint8_t> trustcache;
    // The Cryptex1 personalization ticket
    std::vector<uint8_t> im4m;
    // `Cryptex1,CryptexInfoPlist`, which names and versions the cryptex
    std::vector<uint8_t> info;
    // `Cryptex1,GenericVolume` root hash
    std::vector<uint8_t> volumehash;
    // The `Cryptex1,*` parameters from the build identity. Borrowed, not owned.
    plist_t              cryptex1_properties{};
    // The DeveloperDiskImage defaults Xcode uses
    int64_t              image_type_index{IDEVICE_CRYPTEXD_DDI_IMAGE_TYPE_INDEX};
    uint64_t             persistence{IDEVICE_CRYPTEXD_DDI_PERSISTENCE};
    uint64_t             nonce_persistence{IDEVICE_CRYPTEXD_DDI_NONCE_PERSISTENCE};
    uint64_t             auth{};
};

// The payloads a Cryptex1 DeveloperDiskImage install needs.
class Cryptex1Assets {
  public:
    // Loads the payloads out of an unpacked DDI `Restore` directory
    static Result<Cryptex1Assets, FfiError> load(const std::string& restore_dir);

    // Builds them from buffers the caller already has. `build_identity` is
    // borrowed for the duration of the call.
    static Result<Cryptex1Assets, FfiError> from_parts(const std::vector<uint8_t>& image,
                                                       const std::vector<uint8_t>& trustcache,
                                                       const std::vector<uint8_t>& info,
                                                       const std::vector<uint8_t>& volumehash,
                                                       plist_t                     build_identity);

    // The handle of the nonce domain these assets are personalized against
    Result<uint64_t, FfiError>              nonce_domain() const;

    // RAII / moves
    ~Cryptex1Assets() noexcept                             = default;
    Cryptex1Assets(Cryptex1Assets&&) noexcept              = default;
    Cryptex1Assets& operator=(Cryptex1Assets&&) noexcept   = default;
    Cryptex1Assets(const Cryptex1Assets&)                  = delete;
    Cryptex1Assets&       operator=(const Cryptex1Assets&) = delete;

    Cryptex1AssetsHandle* raw() const noexcept { return handle_.get(); }
    static Cryptex1Assets adopt(Cryptex1AssetsHandle* h) noexcept { return Cryptex1Assets(h); }

  private:
    explicit Cryptex1Assets(Cryptex1AssetsHandle* h) noexcept : handle_(h) {}
    Cryptex1AssetsPtr handle_{};
};

// `cryptexd` over RemoteXPC.
//
// The daemon serves one routine per connection, so every call below consumes
// the client: it is spent afterwards, even when the call fails, and a new one
// must be connected for the next routine.
class Cryptexd {
  public:
    // Factory: connect via RSD (borrows adapter & handshake)
    static Result<Cryptexd, FfiError> connect_rsd(Adapter& adapter, RsdHandshake& rsd);

    // Factory: from socket Box<dyn ReadWrite> (consumes it).
    static Result<Cryptexd, FfiError> from_readwrite_ptr(ReadWriteOpaque* consumed);

    // nice ergonomic overload: consume a C++ ReadWrite by releasing it
    static Result<Cryptexd, FfiError> from_readwrite(ReadWrite&& rw);

    // The device's AppleImage4 chip instance, which identifies it in a Cryptex1
    // personalization request. The caller owns the returned plist.
    Result<plist_t, FfiError>              read_personalization_identifiers();

    // The cryptexes installed on the device
    Result<std::vector<InstalledCryptex>, FfiError> copy_installed();

    // A nonce domain's nonce structure. Use cryptex_nonce() for the nonce TSS
    // wants.
    Result<std::vector<uint8_t>, FfiError> get_nonce(NonceDomain domain);
    Result<std::vector<uint8_t>, FfiError> cryptex_nonce(uint64_t nonce_domain_handle);

    // Rolls a nonce domain's nonce, invalidating anything personalized against
    // the previous one
    Result<void, FfiError>                 roll_nonce(NonceDomain domain);

    // Uninstalls a cryptex by the identifier copy_installed() reports,
    // optionally scoped to one version
    Result<void, FfiError>                 uninstall(const std::string&         identifier,
                                                     const Option<std::string>& version = None);

    // Installs a cryptex
    Result<void, FfiError>                 install(const CryptexInstallRequest& request);

    // RAII / moves
    ~Cryptexd() noexcept                       = default;
    Cryptexd(Cryptexd&&) noexcept              = default;
    Cryptexd& operator=(Cryptexd&&) noexcept   = default;
    Cryptexd(const Cryptexd&)                  = delete;
    Cryptexd&       operator=(const Cryptexd&) = delete;

    CryptexdHandle* raw() const noexcept { return handle_.get(); }
    static Cryptexd adopt(CryptexdHandle* h) noexcept { return Cryptexd(h); }

  private:
    explicit Cryptexd(CryptexdHandle* h) noexcept : handle_(h) {}
    CryptexdPtr handle_{};
};

// Extracts the nonce out of cryptexd's nonce structure.
Result<std::vector<uint8_t>, FfiError> cryptexd_unwrap_nonce(const std::vector<uint8_t>& blob);

// Personalizes and installs the DeveloperDiskImage cryptex end to end: the
// cryptex equivalent of the image mounter's auto-mount. Each step opens its own
// connection off the adapter.
Result<InstalledCryptex, FfiError>
cryptexd_install_ddi(Adapter& adapter, RsdHandshake& rsd, Cryptex1Assets& assets);

// The installed DeveloperDiskImage cryptex, if there is one.
Result<Option<InstalledCryptex>, FfiError> cryptexd_installed_ddi(Adapter&      adapter,
                                                                  RsdHandshake& rsd);

} // namespace IdeviceFFI
