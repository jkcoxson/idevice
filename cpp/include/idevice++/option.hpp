// So here's the thing, std::optional and friends weren't added until C++17.
// Some consumers of this codebase aren't on C++17 yet, so this won't work.
// Plus, as a professional Rust evangelist, it's my duty to place as many Rust
// idioms into other languages as possible to give everyone a taste of greatness.
// Required error handling is correct error handling. And they called me a mad man.

// Heavily influced from https://github.com/oktal/result, thank you

#pragma once

#include <stdexcept>
#include <type_traits>
#include <utility>

namespace IdeviceFFI {

struct none_t {};
constexpr none_t None{};

template <typename T> class Option {
    bool                                                       has_;
    typename std::aligned_storage<sizeof(T), alignof(T)>::type storage_;

    T*       ptr() { return reinterpret_cast<T*>(&storage_); }
    const T* ptr() const { return reinterpret_cast<const T*>(&storage_); }

  public:
    // None
    Option() noexcept : has_(false) {}
    Option(none_t) noexcept : has_(false) {}

    // Some
    Option(const T& v) : has_(true) { ::new (ptr()) T(v); }
    Option(T&& v) : has_(true) { ::new (ptr()) T(std::move(v)); }

    // Copy / move
    Option(const Option& o) : has_(o.has_) {
        if (has_) {
            ::new (ptr()) T(*o.ptr());
        }
    }
    Option(Option&& o) noexcept(std::is_nothrow_move_constructible<T>::value) : has_(o.has_) {
        if (has_) {
            ::new (ptr()) T(std::move(*o.ptr()));
            o.reset();
        }
    }

    Option& operator=(Option o) noexcept(std::is_nothrow_move_constructible<T>::value
                                         && std::is_nothrow_move_assignable<T>::value) {
        swap(o);
        return *this;
    }

    ~Option() { reset(); }

    void reset() noexcept {
        if (has_) {
            ptr()->~T();
            has_ = false;
        }
    }

    void swap(Option& other) noexcept(std::is_nothrow_move_constructible<T>::value) {
        if (has_ && other.has_) {
            using std::swap;
            swap(*ptr(), *other.ptr());
        } else if (has_ && !other.has_) {
            ::new (other.ptr()) T(std::move(*ptr()));
            other.has_ = true;
            reset();
        } else if (!has_ && other.has_) {
            ::new (ptr()) T(std::move(*other.ptr()));
            has_ = true;
            other.reset();
        }
    }

    // State
    bool is_some() const noexcept { return has_; }
    bool is_none() const noexcept { return !has_; }

    // Unwraps (ref-qualified)
    T&   unwrap() & {
        if (!has_) {
            throw std::runtime_error("unwrap on None");
        }
        return *ptr();
    }
    const T& unwrap() const& {
        if (!has_) {
            throw std::runtime_error("unwrap on None");
        }
        return *ptr();
    }
    T unwrap() && {
        if (!has_) {
            throw std::runtime_error("unwrap on None");
        }
        T tmp = std::move(*ptr());
        reset();
        return tmp;
    }

    // unwrap_or / unwrap_or_else
    T unwrap_or(T default_value) const& { return has_ ? *ptr() : std::move(default_value); }
    T unwrap_or(T default_value) && { return has_ ? std::move(*ptr()) : std::move(default_value); }
    T unwrap_or(const T& default_value) const& { return has_ ? *ptr() : default_value; }
    T unwrap_or(T&& default_value) const& { return has_ ? *ptr() : std::move(default_value); }

    template <typename F> T unwrap_or_else(F&& f) const& {
        return has_ ? *ptr() : static_cast<T>(f());
    }
    template <typename F> T unwrap_or_else(F&& f) && {
        return has_ ? std::move(*ptr()) : static_cast<T>(f());
    }

    // map
    template <typename F>
    auto map(F&& f) const -> Option<typename std::decay<decltype(f(*ptr()))>::type> {
        using U = typename std::decay<decltype(f(*ptr()))>::type;
        if (has_) {
            return Option<U>(f(*ptr()));
        }
        return Option<U>(None);
    }

    template <typename F>
    auto map(F&& f) && -> Option<typename std::decay<decltype(f(std::move(*ptr())))>::type> {
        using U = typename std::decay<decltype(f(std::move(*ptr())))>::type;
        if (has_) {
            // Move the value into the function
            return Option<U>(f(std::move(*ptr())));
        }
        return Option<U>(None);
    }
};

// Helpers
template <typename T> inline Option<typename std::decay<T>::type> Some(T&& v) {
    return Option<typename std::decay<T>::type>(std::forward<T>(v));
}
inline Option<void> Some() = delete; // no Option<void>

#define match_option(opt, some_name, some_block, none_block)                                       \
    /* NOTE: you may return in a block, but not break/continue */                                  \
    do {                                                                                           \
        auto&& _option_val = (opt);                                                                \
        if (_option_val.is_some()) {                                                               \
            auto&& some_name = _option_val.unwrap();                                               \
            some_block                                                                             \
        } else {                                                                                   \
            none_block                                                                             \
        }                                                                                          \
    } while (0)

// --- Option helpers: if_let_some / if_let_some_move / if_let_none ---

#define _opt_concat(a, b) a##b
#define _opt_unique(base) _opt_concat(base, __LINE__)

/* Bind a reference to the contained value if Some(...) */
#define if_let_some(expr, name, block)                                                             \
    /* NOTE: you may return in a block, but not break/continue */                                  \
    do {                                                                                           \
        auto _opt_unique(_opt_) = (expr);                                                          \
        if (_opt_unique(_opt_).is_some()) {                                                        \
            auto&& name = _opt_unique(_opt_).unwrap();                                             \
            block                                                                                  \
        }                                                                                          \
    } while (0)

/* Move the contained value out (consumes the Option) if Some(...) */
#define if_let_some_move(expr, name, block)                                                        \
    /* NOTE: you may return in a block, but not break/continue */                                  \
    do {                                                                                           \
        auto _opt_unique(_opt_) = (expr);                                                          \
        if (_opt_unique(_opt_).is_some()) {                                                        \
            auto name = std::move(_opt_unique(_opt_)).unwrap();                                    \
            block                                                                                  \
        }                                                                                          \
    } while (0)

} // namespace IdeviceFFI
