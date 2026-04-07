// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <vector>

namespace IdeviceFFI {

using ApplicationListingPtr =
    std::unique_ptr<ApplicationListingHandle,
                    FnDeleter<ApplicationListingHandle, application_listing_free>>;

class ApplicationListing {
  public:
    static Result<ApplicationListing, FfiError> create(RemoteServer& server);

    /// Returns a vector of plist_t dictionaries, one per app.
    /// Each dictionary contains the app's metadata (DisplayName, Version, etc.).
    /// Caller must free each plist_t with plist_free().
    Result<std::vector<plist_t>, FfiError> installed_applications();

    ~ApplicationListing() noexcept                               = default;
    ApplicationListing(ApplicationListing&&) noexcept            = default;
    ApplicationListing& operator=(ApplicationListing&&) noexcept = default;
    ApplicationListing(const ApplicationListing&)                = delete;
    ApplicationListing&       operator=(const ApplicationListing&) = delete;

    ApplicationListingHandle* raw() const noexcept { return handle_.get(); }
    static ApplicationListing adopt(ApplicationListingHandle* h) noexcept {
        return ApplicationListing(h);
    }

  private:
    explicit ApplicationListing(ApplicationListingHandle* h) noexcept : handle_(h) {}
    ApplicationListingPtr handle_{};
};

} // namespace IdeviceFFI
