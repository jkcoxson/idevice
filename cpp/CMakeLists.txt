cmake_minimum_required(VERSION 3.16)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Project
project(idevice++
  VERSION 0.1.0
  LANGUAGES CXX
)

# ---- Options ---------------------------------------------------------------

# Path to the Rust static library (override on the command line if needed):
#   cmake -DIDEVICE_FFI_PATH=/absolute/path/to/libidevice_ffi.a ..
set(IDEVICE_FFI_PATH
    "${CMAKE_CURRENT_SOURCE_DIR}/../target/release/libidevice_ffi.a"
    CACHE FILEPATH "Path to libidevice_ffi.a produced by Rust build"
)

# Optional extra libraries/frameworks if your Rust/C bridge needs them:
#   e.g. -framework CoreFoundation on macOS, etc.
set(IDEVICEPP_EXTRA_LIBS
    ""
    CACHE STRING "Extra libraries to link (space-separated)"
)

# ---- Tooling ---------------------------------------------------------------

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)  # Helpful for shared libs

# Threads
find_package(Threads REQUIRED)

# On some platforms dl is required for Rust staticlibs that use libdl
include(CheckLibraryExists)
set(HAVE_LIBDL FALSE)
if(UNIX AND NOT APPLE)
  check_library_exists(dl dlopen "" HAVE_LIBDL)
endif()

# ---- Imported Rust static library ------------------------------------------

if(NOT EXISTS "${IDEVICE_FFI_PATH}")
  message(FATAL_ERROR "IDEVICE_FFI_PATH does not exist: ${IDEVICE_FFI_PATH}")
endif()

add_library(idevice_ffi STATIC IMPORTED GLOBAL)
set_target_properties(idevice_ffi PROPERTIES
  IMPORTED_LOCATION "${IDEVICE_FFI_PATH}"
)

configure_file("${CMAKE_CURRENT_SOURCE_DIR}/../ffi/idevice.h" "${CMAKE_CURRENT_SOURCE_DIR}/include" COPYONLY)

# ---- Our C++ library -------------------------------------------------------

# Collect sources (convenience: tracks new files when you re-run CMake)
file(GLOB_RECURSE IDEVICEPP_SOURCES
  CONFIGURE_DEPENDS
  "${CMAKE_CURRENT_SOURCE_DIR}/src/*.cc"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/*.cpp"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/*.cxx"
)

# Respect BUILD_SHARED_LIBS (OFF=static, ON=shared)
add_library(${PROJECT_NAME})
target_sources(${PROJECT_NAME} PRIVATE ${IDEVICEPP_SOURCES})

# Public headers live under include/, e.g. include/idevice++/Foo.hpp
target_include_directories(${PROJECT_NAME}
  PUBLIC
    "$<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>"
    "$<INSTALL_INTERFACE:include>"
)

# Link dependencies
target_link_libraries(${PROJECT_NAME}
  PUBLIC
    idevice_ffi
    Threads::Threads
)

# Link libdl on Linux if present
if(HAVE_LIBDL)
  target_link_libraries(${PROJECT_NAME} PUBLIC dl)
endif()

# Windows winsock (if you ever build there)
if(WIN32)
  target_link_libraries(${PROJECT_NAME} PUBLIC ws2_32)
endif()

# Extra user-specified libs/frameworks
if(IDEVICEPP_EXTRA_LIBS)
  # Split by spaces and link each one
  separate_arguments(_extra_libs NATIVE_COMMAND ${IDEVICEPP_EXTRA_LIBS})
  target_link_libraries(${PROJECT_NAME} PUBLIC ${_extra_libs})
endif()

# Visibility / warnings (tweak to taste)
if(MSVC)
  target_compile_options(${PROJECT_NAME} PRIVATE /permissive- /W4 /WX-)
else()
  target_compile_options(${PROJECT_NAME} PRIVATE -Wall -Wextra -Wpedantic)
endif()

# ---- Examples (optional) ---------------------------------------------------

option(BUILD_EXAMPLES "Build examples in examples/ directory" OFF)
if(BUILD_EXAMPLES)
  file(GLOB EXAMPLE_SOURCES CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/examples/*.cpp")
  foreach(ex_src IN LISTS EXAMPLE_SOURCES)
    get_filename_component(exe_name "${ex_src}" NAME_WE)
    add_executable(${exe_name} "${ex_src}")
    target_link_libraries(${exe_name} PRIVATE ${PROJECT_NAME})
    target_include_directories(${exe_name} PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/include")
  endforeach()
endif()

