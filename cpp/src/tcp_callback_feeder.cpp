// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/tcp_object_stack.hpp>

namespace IdeviceFFI {

// ---------- TcpFeeder ----------
bool TcpObjectStackFeeder::write(const uint8_t* data, std::size_t len, FfiError& err) const {
    if (IdeviceFfiError* e = ::idevice_tcp_feed_object_write(h_, data, len)) {
        err = FfiError(e);
        return false;
    }
    return true;
}

// ---------- TcpEater ----------
bool TcpObjectStackEater::read(OwnedBuffer& out, FfiError& err) const {
    uint8_t*    ptr = nullptr;
    std::size_t len = 0;
    if (IdeviceFfiError* e = ::idevice_tcp_eat_object_read(h_, &ptr, &len)) {
        err = FfiError(e);
        return false;
    }
    // Success: adopt the buffer (freed via idevice_data_free in OwnedBuffer dtor)
    out.adopt(ptr, len);
    return true;
}

// ---------- TcpStackFromCallback ----------
std::optional<TcpObjectStack>
TcpObjectStack::create(const std::string& our_ip, const std::string& their_ip, FfiError& err) {
    ::TcpFeedObject* feeder_h  = nullptr;
    ::TcpEatObject*  eater_h   = nullptr;
    ::AdapterHandle* adapter_h = nullptr;

    if (IdeviceFfiError* e = ::idevice_tcp_stack_into_sync_objects(
            our_ip.c_str(), their_ip.c_str(), &feeder_h, &eater_h, &adapter_h)) {
        err = FfiError(e);
        return std::nullopt;
    }

    auto impl     = std::make_unique<Impl>();
    impl->feeder  = TcpObjectStackFeeder(feeder_h);
    impl->eater   = TcpObjectStackEater(eater_h);
    impl->adapter = Adapter::adopt(adapter_h);

    TcpObjectStack out;
    out.impl_ = std::move(impl);
    return out;
}

TcpObjectStackFeeder& TcpObjectStack::feeder() {
    return impl_->feeder;
}
const TcpObjectStackFeeder& TcpObjectStack::feeder() const {
    return impl_->feeder;
}

TcpObjectStackEater& TcpObjectStack::eater() {
    return impl_->eater;
}
const TcpObjectStackEater& TcpObjectStack::eater() const {
    return impl_->eater;
}

Adapter& TcpObjectStack::adapter() {
    if (!impl_ || !impl_->adapter) {
        static Adapter* never = nullptr;
        return *never;
    }
    return *(impl_->adapter);
}
const Adapter& TcpObjectStack::adapter() const {
    if (!impl_ || !impl_->adapter) {
        static Adapter* never = nullptr;
        return *never;
    }
    return *(impl_->adapter);
}

// ---------- Release APIs ----------
std::optional<TcpObjectStackFeeder> TcpObjectStack::release_feeder() {
    if (!impl_)
        return std::nullopt;
    auto has = impl_->feeder.raw() != nullptr;
    if (!has)
        return std::nullopt;
    TcpObjectStackFeeder out = std::move(impl_->feeder);
    // impl_->feeder is now empty (h_ == nullptr) thanks to move
    return std::optional<TcpObjectStackFeeder>(std::move(out));
}

std::optional<TcpObjectStackEater> TcpObjectStack::release_eater() {
    if (!impl_)
        return std::nullopt;
    auto has = impl_->eater.raw() != nullptr;
    if (!has)
        return std::nullopt;
    TcpObjectStackEater out = std::move(impl_->eater);
    return std::optional<TcpObjectStackEater>(std::move(out));
}

std::optional<Adapter> TcpObjectStack::release_adapter() {
    if (!impl_ || !impl_->adapter)
        return std::nullopt;
    // Move out and clear our optional
    auto out = std::move(*(impl_->adapter));
    impl_->adapter.reset();
    return std::optional<Adapter>(std::move(out));
}

} // namespace IdeviceFFI
