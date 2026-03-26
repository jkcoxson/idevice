// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/house_arrest.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<HouseArrest, FfiError> HouseArrest::connect(Provider& provider) {
    HouseArrestClientHandle* out = nullptr;
    FfiError                 e(::house_arrest_client_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(HouseArrest::adopt(out));
}

Result<HouseArrest, FfiError> HouseArrest::from_socket(Idevice&& socket) {
    HouseArrestClientHandle* out = nullptr;
    FfiError                 e(::house_arrest_client_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(HouseArrest::adopt(out));
}

Result<AfcClientHandle*, FfiError> HouseArrest::vend_container(const std::string& bundle_id) {
    AfcClientHandle* afc_out = nullptr;
    FfiError         e(::house_arrest_vend_container(handle_.get(), bundle_id.c_str(), &afc_out));
    if (e) {
        return Err(e);
    }
    return Ok(afc_out);
}

Result<AfcClientHandle*, FfiError> HouseArrest::vend_documents(const std::string& bundle_id) {
    AfcClientHandle* afc_out = nullptr;
    FfiError         e(::house_arrest_vend_documents(handle_.get(), bundle_id.c_str(), &afc_out));
    if (e) {
        return Err(e);
    }
    return Ok(afc_out);
}

} // namespace IdeviceFFI
