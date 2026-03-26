// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/misagent.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<Misagent, FfiError> Misagent::connect(Provider& provider) {
    MisagentClientHandle* out = nullptr;
    FfiError              e(::misagent_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(Misagent::adopt(out));
}

Result<void, FfiError> Misagent::install(const uint8_t* profile_data, size_t profile_len) {
    FfiError e(::misagent_install(handle_.get(), profile_data, profile_len));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> Misagent::remove(const std::string& profile_id) {
    FfiError e(::misagent_remove(handle_.get(), profile_id.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<std::vector<std::vector<uint8_t>>, FfiError> Misagent::copy_all() {
    uint8_t** profiles     = nullptr;
    size_t*   profiles_len = nullptr;
    size_t    count        = 0;

    FfiError  e(::misagent_copy_all(handle_.get(), &profiles, &profiles_len, &count));
    if (e) {
        return Err(e);
    }

    std::vector<std::vector<uint8_t>> result;
    if (profiles && profiles_len) {
        result.reserve(count);
        for (size_t i = 0; i < count; ++i) {
            if (profiles[i] && profiles_len[i] > 0) {
                result.emplace_back(profiles[i], profiles[i] + profiles_len[i]);
            } else {
                result.emplace_back();
            }
        }
        ::misagent_free_profiles(profiles, profiles_len, count);
    }

    return Ok(std::move(result));
}

} // namespace IdeviceFFI
