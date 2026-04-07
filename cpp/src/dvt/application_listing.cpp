// Jackson Coxson

#include <idevice++/dvt/application_listing.hpp>

namespace IdeviceFFI {

Result<ApplicationListing, FfiError> ApplicationListing::create(RemoteServer& server) {
    ApplicationListingHandle* out = nullptr;
    FfiError                  e(::application_listing_new(server.raw(), &out));
    if (e) return Err(e);
    return Ok(ApplicationListing::adopt(out));
}

Result<std::vector<plist_t>, FfiError> ApplicationListing::installed_applications() {
    plist_t* ptrs  = nullptr;
    size_t   count = 0;
    FfiError e(::application_listing_get_apps(handle_.get(), &ptrs, &count));
    if (e) return Err(e);

    std::vector<plist_t> result;
    result.reserve(count);
    for (size_t i = 0; i < count; ++i) {
        result.push_back(ptrs[i]);
    }
    // Free the outer array only; caller owns the individual plist_t values.
    ::idevice_outer_slice_free(reinterpret_cast<void*>(ptrs), count);
    return Ok(std::move(result));
}

} // namespace IdeviceFFI
